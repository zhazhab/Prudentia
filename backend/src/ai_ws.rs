use std::collections::HashMap;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use tokio::{sync::mpsc, task::JoinHandle};

use crate::{
    portfolio::{self, PortfolioImageImportPreviewRequest},
    state::AppState,
};

const MAX_ACTIVE_TASKS_PER_CONNECTION: usize = 2;
const PORTFOLIO_IMAGE_IMPORT_ARTIFACT: &str = "portfolio_image_import.preview";

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum AiWsClientMessage {
    #[serde(rename = "portfolio_image_import.start")]
    PortfolioImageImportStart {
        request_id: String,
        payload: PortfolioImageImportPreviewRequest,
    },
    #[serde(rename = "cancel")]
    Cancel { request_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AiWsServerMessage {
    #[serde(rename = "accepted")]
    Accepted { request_id: String },
    #[serde(rename = "progress")]
    Progress { request_id: String, stage: String },
    #[serde(rename = "completed")]
    Completed {
        request_id: String,
        artifact_type: String,
        data: serde_json::Value,
    },
    #[serde(rename = "failed")]
    Failed {
        request_id: String,
        code: String,
        error: String,
    },
    #[serde(rename = "canceled")]
    Canceled { request_id: String },
}

enum SocketEvent {
    Send(AiWsServerMessage),
    Finished(String),
}

pub async fn ws_handler(State(state): State<AppState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let (events_tx, mut events_rx) = mpsc::unbounded_channel::<SocketEvent>();
    let mut active_tasks: HashMap<String, JoinHandle<()>> = HashMap::new();

    loop {
        tokio::select! {
            Some(event) = events_rx.recv() => {
                match event {
                    SocketEvent::Send(message) => {
                        if !send_message(&mut socket, message).await {
                            break;
                        }
                    }
                    SocketEvent::Finished(request_id) => {
                        active_tasks.remove(&request_id);
                    }
                }
            }
            incoming = socket.recv() => {
                let Some(incoming) = incoming else {
                    break;
                };

                match incoming {
                    Ok(Message::Text(text)) => {
                        handle_client_text(
                            text.to_string(),
                            &state,
                            &events_tx,
                            &mut active_tasks,
                        );
                    }
                    Ok(Message::Close(_)) => break,
                    Ok(Message::Ping(_)) | Ok(Message::Pong(_)) | Ok(Message::Binary(_)) => {}
                    Err(error) => {
                        tracing::debug!(error = %error, "AI websocket receive failed");
                        break;
                    }
                }
            }
        }
    }

    for (_, handle) in active_tasks {
        handle.abort();
    }
}

fn handle_client_text(
    text: String,
    state: &AppState,
    events_tx: &mpsc::UnboundedSender<SocketEvent>,
    active_tasks: &mut HashMap<String, JoinHandle<()>>,
) {
    let message = match serde_json::from_str::<AiWsClientMessage>(&text) {
        Ok(message) => message,
        Err(error) => {
            send_socket_event(
                events_tx,
                AiWsServerMessage::Failed {
                    request_id: String::new(),
                    code: "invalid_message".to_string(),
                    error: sanitize_error(error.to_string()),
                },
            );
            return;
        }
    };

    match message {
        AiWsClientMessage::PortfolioImageImportStart {
            request_id,
            payload,
        } => start_portfolio_image_import(
            request_id,
            payload,
            state.clone(),
            events_tx,
            active_tasks,
        ),
        AiWsClientMessage::Cancel { request_id } => {
            if let Some(handle) = active_tasks.remove(&request_id) {
                handle.abort();
            }
            send_socket_event(events_tx, AiWsServerMessage::Canceled { request_id });
        }
    }
}

fn start_portfolio_image_import(
    request_id: String,
    payload: PortfolioImageImportPreviewRequest,
    state: AppState,
    events_tx: &mpsc::UnboundedSender<SocketEvent>,
    active_tasks: &mut HashMap<String, JoinHandle<()>>,
) {
    if active_tasks.contains_key(&request_id) {
        send_socket_event(
            events_tx,
            AiWsServerMessage::Failed {
                request_id,
                code: "duplicate_request".to_string(),
                error: "request_id is already active".to_string(),
            },
        );
        return;
    }

    if active_tasks.len() >= MAX_ACTIVE_TASKS_PER_CONNECTION {
        send_socket_event(
            events_tx,
            AiWsServerMessage::Failed {
                request_id,
                code: "too_many_active_tasks".to_string(),
                error: "too many active AI tasks on this connection".to_string(),
            },
        );
        return;
    }

    send_socket_event(
        events_tx,
        AiWsServerMessage::Accepted {
            request_id: request_id.clone(),
        },
    );

    let task_request_id = request_id.clone();
    let task_tx = events_tx.clone();
    let handle = tokio::spawn(async move {
        run_portfolio_image_import_task(task_request_id, state, payload, task_tx).await;
    });
    active_tasks.insert(request_id, handle);
}

async fn run_portfolio_image_import_task(
    request_id: String,
    state: AppState,
    payload: PortfolioImageImportPreviewRequest,
    events_tx: mpsc::UnboundedSender<SocketEvent>,
) {
    let progress_tx = events_tx.clone();
    let progress_request_id = request_id.clone();
    let result =
        portfolio::preview_image_import_with_progress(state.ai.clone(), payload, move |stage| {
            let tx = progress_tx.clone();
            let request_id = progress_request_id.clone();
            async move {
                send_socket_event(
                    &tx,
                    AiWsServerMessage::Progress {
                        request_id,
                        stage: stage.to_string(),
                    },
                );
            }
        })
        .await;

    match result {
        Ok(preview) => {
            let data = serde_json::to_value(preview).unwrap_or_else(|error| {
                serde_json::json!({
                    "serialization_error": error.to_string()
                })
            });
            send_socket_event(
                &events_tx,
                AiWsServerMessage::Completed {
                    request_id: request_id.clone(),
                    artifact_type: PORTFOLIO_IMAGE_IMPORT_ARTIFACT.to_string(),
                    data,
                },
            );
        }
        Err(error) => send_socket_event(
            &events_tx,
            AiWsServerMessage::Failed {
                request_id: request_id.clone(),
                code: "portfolio_image_import_failed".to_string(),
                error: sanitize_error(error.to_string()),
            },
        ),
    }

    let _ = events_tx.send(SocketEvent::Finished(request_id));
}

fn send_socket_event(events_tx: &mpsc::UnboundedSender<SocketEvent>, message: AiWsServerMessage) {
    let _ = events_tx.send(SocketEvent::Send(message));
}

async fn send_message(socket: &mut WebSocket, message: AiWsServerMessage) -> bool {
    let text = match serde_json::to_string(&message) {
        Ok(text) => text,
        Err(error) => {
            tracing::warn!(error = %error, "failed to serialize AI websocket message");
            return true;
        }
    };

    match socket.send(Message::Text(text.into())).await {
        Ok(()) => true,
        Err(error) => {
            tracing::debug!(error = %error, "AI websocket send failed");
            false
        }
    }
}

fn sanitize_error(error: String) -> String {
    let mut lines = error.lines().take(4).collect::<Vec<_>>().join("\n");
    if lines.len() > 600 {
        lines.truncate(600);
        lines.push_str("...");
    }
    lines
}
