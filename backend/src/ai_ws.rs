use std::{
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

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
static NEXT_CONNECTION_ID: AtomicU64 = AtomicU64::new(1);

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
    let connected_at = Instant::now();
    let connection_id = NEXT_CONNECTION_ID.fetch_add(1, Ordering::Relaxed);

    tracing::info!(
        connection_id,
        active_tasks = active_tasks.len(),
        "AI websocket connection established"
    );

    loop {
        tokio::select! {
            Some(event) = events_rx.recv() => {
                match event {
                    SocketEvent::Send(message) => {
                        if !send_message(&mut socket, message, connection_id).await {
                            break;
                        }
                    }
                    SocketEvent::Finished(request_id) => {
                        tracing::debug!(
                            connection_id,
                            request_id = %request_id,
                            active_tasks = active_tasks.len().saturating_sub(1),
                            "AI websocket task removed from active set"
                        );
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
                        tracing::debug!(
                            connection_id,
                            bytes = text.len(),
                            active_tasks = active_tasks.len(),
                            "AI websocket client message received"
                        );
                        handle_client_text(
                            text.to_string(),
                            &state,
                            &events_tx,
                            &mut active_tasks,
                            connection_id,
                        );
                    }
                    Ok(Message::Close(_)) => {
                        tracing::info!(
                            connection_id,
                            active_tasks = active_tasks.len(),
                            connected_ms = connected_at.elapsed().as_millis(),
                            "AI websocket close frame received"
                        );
                        break;
                    }
                    Ok(Message::Ping(_)) | Ok(Message::Pong(_)) | Ok(Message::Binary(_)) => {}
                    Err(error) => {
                        tracing::debug!(
                            connection_id,
                            error = %error,
                            "AI websocket receive failed"
                        );
                        break;
                    }
                }
            }
        }
    }

    let aborted_tasks = active_tasks.len();
    for (_, handle) in active_tasks {
        handle.abort();
    }
    tracing::info!(
        connection_id,
        connected_ms = connected_at.elapsed().as_millis(),
        aborted_tasks,
        "AI websocket disconnected"
    );
}

fn handle_client_text(
    text: String,
    state: &AppState,
    events_tx: &mpsc::UnboundedSender<SocketEvent>,
    active_tasks: &mut HashMap<String, JoinHandle<()>>,
    connection_id: u64,
) {
    let message = match serde_json::from_str::<AiWsClientMessage>(&text) {
        Ok(message) => message,
        Err(error) => {
            tracing::warn!(
                connection_id,
                error = %sanitize_error(error.to_string()),
                "AI websocket client message decode failed"
            );
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

    let (request_id, message_type) = client_message_summary(&message);
    tracing::info!(
        connection_id,
        request_id = %request_id,
        message_type,
        active_tasks = active_tasks.len(),
        "AI websocket client message decoded"
    );

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
            connection_id,
        ),
        AiWsClientMessage::Cancel { request_id } => {
            let canceled_running_task = if let Some(handle) = active_tasks.remove(&request_id) {
                handle.abort();
                true
            } else {
                false
            };
            tracing::info!(
                connection_id,
                request_id = %request_id,
                canceled_running_task,
                active_tasks = active_tasks.len(),
                "AI websocket task cancel requested"
            );
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
    connection_id: u64,
) {
    if active_tasks.contains_key(&request_id) {
        tracing::warn!(
            connection_id,
            request_id = %request_id,
            "AI websocket task rejected because request_id is already active"
        );
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
        tracing::warn!(
            connection_id,
            request_id = %request_id,
            active_tasks = active_tasks.len(),
            max_active_tasks = MAX_ACTIVE_TASKS_PER_CONNECTION,
            "AI websocket task rejected because active task limit was reached"
        );
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

    let active_tasks_after_accept = active_tasks.len() + 1;
    tracing::info!(
        connection_id,
        request_id = %request_id,
        file_name = %payload.file_name,
        mime_type = payload.mime_type.as_deref().unwrap_or("unknown"),
        active_tasks = active_tasks_after_accept,
        "AI websocket portfolio image import accepted"
    );
    send_socket_event(
        events_tx,
        AiWsServerMessage::Accepted {
            request_id: request_id.clone(),
        },
    );

    let task_request_id = request_id.clone();
    let task_tx = events_tx.clone();
    let handle = tokio::spawn(async move {
        run_portfolio_image_import_task(task_request_id, state, payload, task_tx, connection_id)
            .await;
    });
    active_tasks.insert(request_id, handle);
}

async fn run_portfolio_image_import_task(
    request_id: String,
    state: AppState,
    payload: PortfolioImageImportPreviewRequest,
    events_tx: mpsc::UnboundedSender<SocketEvent>,
    connection_id: u64,
) {
    let started_at = Instant::now();
    let progress_tx = events_tx.clone();
    let progress_request_id = request_id.clone();
    let result = portfolio::preview_image_import_with_progress(
        Some(state.pool.clone()),
        state.ai.clone(),
        payload,
        move |stage| {
            let tx = progress_tx.clone();
            let request_id = progress_request_id.clone();
            let elapsed_ms = started_at.elapsed().as_millis();
            async move {
                tracing::info!(
                    connection_id,
                    request_id = %request_id,
                    stage,
                    elapsed_ms,
                    "portfolio image import stage"
                );
                send_socket_event(
                    &tx,
                    AiWsServerMessage::Progress {
                        request_id,
                        stage: stage.to_string(),
                    },
                );
            }
        },
    )
    .await;

    match result {
        Ok(preview) => {
            let row_count = preview.draft_rows.len();
            let warning_count = preview.warnings.len();
            let error_count = preview
                .draft_rows
                .iter()
                .map(|row| row.errors.len())
                .sum::<usize>();
            let data = serde_json::to_value(preview).unwrap_or_else(|error| {
                serde_json::json!({
                    "serialization_error": error.to_string()
                })
            });
            tracing::info!(
                connection_id,
                request_id = %request_id,
                elapsed_ms = started_at.elapsed().as_millis(),
                row_count,
                warning_count,
                error_count,
                "portfolio image import completed"
            );
            send_socket_event(
                &events_tx,
                AiWsServerMessage::Completed {
                    request_id: request_id.clone(),
                    artifact_type: PORTFOLIO_IMAGE_IMPORT_ARTIFACT.to_string(),
                    data,
                },
            );
        }
        Err(error) => {
            let sanitized_error = sanitize_error(error.to_string());
            tracing::warn!(
                connection_id,
                request_id = %request_id,
                elapsed_ms = started_at.elapsed().as_millis(),
                error = %sanitized_error,
                "portfolio image import failed"
            );
            send_socket_event(
                &events_tx,
                AiWsServerMessage::Failed {
                    request_id: request_id.clone(),
                    code: "portfolio_image_import_failed".to_string(),
                    error: sanitized_error,
                },
            );
        }
    }

    let _ = events_tx.send(SocketEvent::Finished(request_id));
}

fn send_socket_event(events_tx: &mpsc::UnboundedSender<SocketEvent>, message: AiWsServerMessage) {
    let (request_id, message_type) = server_message_summary(&message);
    tracing::debug!(
        request_id = %request_id,
        message_type,
        "AI websocket server event queued"
    );
    let _ = events_tx.send(SocketEvent::Send(message));
}

async fn send_message(
    socket: &mut WebSocket,
    message: AiWsServerMessage,
    connection_id: u64,
) -> bool {
    let (request_id, message_type) = server_message_summary(&message);
    let text = match serde_json::to_string(&message) {
        Ok(text) => text,
        Err(error) => {
            tracing::warn!(
                connection_id,
                request_id = %request_id,
                message_type,
                error = %error,
                "failed to serialize AI websocket message"
            );
            return true;
        }
    };

    match socket.send(Message::Text(text.into())).await {
        Ok(()) => {
            tracing::info!(
                connection_id,
                request_id = %request_id,
                message_type,
                "AI websocket server message sent"
            );
            true
        }
        Err(error) => {
            tracing::debug!(
                connection_id,
                request_id = %request_id,
                message_type,
                error = %error,
                "AI websocket send failed"
            );
            false
        }
    }
}

fn client_message_summary(message: &AiWsClientMessage) -> (&str, &'static str) {
    match message {
        AiWsClientMessage::PortfolioImageImportStart { request_id, .. } => {
            (request_id, "portfolio_image_import.start")
        }
        AiWsClientMessage::Cancel { request_id } => (request_id, "cancel"),
    }
}

fn server_message_summary(message: &AiWsServerMessage) -> (&str, &'static str) {
    match message {
        AiWsServerMessage::Accepted { request_id } => (request_id, "accepted"),
        AiWsServerMessage::Progress { request_id, .. } => (request_id, "progress"),
        AiWsServerMessage::Completed { request_id, .. } => (request_id, "completed"),
        AiWsServerMessage::Failed { request_id, .. } => (request_id, "failed"),
        AiWsServerMessage::Canceled { request_id } => (request_id, "canceled"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_message_summary_reports_type_and_request_id() {
        let message = AiWsClientMessage::Cancel {
            request_id: "request-1".to_string(),
        };

        assert_eq!(client_message_summary(&message), ("request-1", "cancel"));
    }

    #[test]
    fn server_message_summary_reports_type_and_request_id() {
        let message = AiWsServerMessage::Progress {
            request_id: "request-2".to_string(),
            stage: "recognizing".to_string(),
        };

        assert_eq!(server_message_summary(&message), ("request-2", "progress"));
    }
}
