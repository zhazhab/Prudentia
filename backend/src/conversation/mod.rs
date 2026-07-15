use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        DefaultBodyLimit, Path, Query, State,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{get, patch, post},
    Json, Router,
};

use crate::{
    error::{AppError, AppResult},
    memo_thread,
    state::AppState,
};

mod actions;
mod attachments;
mod company;
mod context;
mod engine;
mod research;
mod storage;
mod subject_resolution;
mod task_routing;
mod types;

pub use engine::ConversationEngine;
pub use research::provider_from_config as research_provider_from_config;
pub use types::*;

fn is_simple_social_turn(message: &str) -> bool {
    let normalized = message
        .trim()
        .trim_matches(|character: char| {
            character.is_whitespace()
                || matches!(
                    character,
                    '!' | '！'
                        | '?'
                        | '？'
                        | '.'
                        | '。'
                        | ','
                        | '，'
                        | ':'
                        | '：'
                        | ';'
                        | '；'
                        | '~'
                        | '～'
                )
        })
        .to_ascii_lowercase();

    matches!(
        normalized.as_str(),
        "你好"
            | "您好"
            | "你好啊"
            | "嗨"
            | "在吗"
            | "早上好"
            | "下午好"
            | "晚上好"
            | "晚安"
            | "你是谁"
            | "你能做什么"
            | "你可以做什么"
            | "你能干什么"
            | "能干什么"
            | "hello"
            | "hi"
            | "hey"
            | "who are you"
            | "what can you do"
    )
}

fn requests_local_context_only(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    [
        "只根据",
        "仅根据",
        "只使用",
        "仅使用",
        "不要检索",
        "无需检索",
        "不查外部",
        "不使用外部",
        "only use local",
        "only use the existing",
        "without external research",
        "do not research",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/runs", post(start_run))
        .route("/runs/active", get(active_runs))
        .route("/runs/{id}/cancel", post(cancel_run))
        .route("/runs/{id}/retry", post(retry_run))
        .route("/threads", get(list_threads))
        .route("/threads/{id}", get(get_thread).delete(delete_thread))
        .route("/threads/{id}/archive", post(archive_thread))
        .route("/threads/{id}/subject", patch(update_subject))
        .route("/companies/{symbol}/views", get(company_view_history))
        .route(
            "/companies/{symbol}/views/{version}/rollback",
            post(rollback_company_view),
        )
        .route("/actions/{id}", patch(update_action))
        .route("/actions/{id}/confirm", post(confirm_action))
        .route("/actions/{id}/reject", post(reject_action))
        .route(
            "/attachments",
            post(upload_attachment).layer(DefaultBodyLimit::max(30 * 1024 * 1024)),
        )
        .route("/events/ws", get(events_ws))
}

async fn start_run(
    State(state): State<AppState>,
    Json(request): Json<StartRunRequest>,
) -> AppResult<(StatusCode, Json<StartRunResponse>)> {
    Ok((
        StatusCode::ACCEPTED,
        Json(state.conversation.start_run(request).await?),
    ))
}

async fn active_runs(State(state): State<AppState>) -> AppResult<Json<Vec<ConversationRun>>> {
    Ok(Json(state.conversation.active_runs().await?))
}

async fn cancel_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ConversationRun>> {
    Ok(Json(state.conversation.cancel_run(&id).await?))
}

async fn retry_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<(StatusCode, Json<StartRunResponse>)> {
    Ok((
        StatusCode::ACCEPTED,
        Json(state.conversation.retry_run(&id).await?),
    ))
}

async fn list_threads(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<ConversationThreadSummary>>> {
    Ok(Json(state.conversation.list_threads().await?))
}

async fn get_thread(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<ThreadDetailQuery>,
) -> AppResult<Json<ConversationThreadDetail>> {
    Ok(Json(
        state
            .conversation
            .thread_detail(
                &id,
                query.message_limit.unwrap_or(50),
                query.before_message_id.as_deref(),
            )
            .await?,
    ))
}

async fn archive_thread(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<memo_thread::MemoThreadSummary>> {
    Ok(Json(memo_thread::archive(&state.pool, &id).await?))
}

async fn delete_thread(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<memo_thread::MemoThreadSummary>> {
    if let Some(run) = state
        .conversation
        .active_runs()
        .await?
        .into_iter()
        .find(|run| run.thread_id == id)
    {
        state.conversation.cancel_run(&run.id).await?;
    }
    Ok(Json(memo_thread::soft_delete(&state.pool, &id).await?))
}

async fn update_subject(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateSubjectRequest>,
) -> AppResult<Json<ThreadSubject>> {
    Ok(Json(state.conversation.update_subject(&id, request).await?))
}

async fn company_view_history(
    State(state): State<AppState>,
    Path(symbol): Path<String>,
) -> AppResult<Json<Vec<CompanyViewVersion>>> {
    Ok(Json(
        company::list_company_view_versions(state.conversation.pool(), &symbol).await?,
    ))
}

async fn rollback_company_view(
    State(state): State<AppState>,
    Path((symbol, version)): Path<(String, i64)>,
    Json(request): Json<RollbackCompanyViewRequest>,
) -> AppResult<Json<CompanyView>> {
    Ok(Json(
        company::rollback_company_view(
            state.conversation.pool(),
            state.conversation.workspace_dir(),
            &symbol,
            version,
            request.expected_version,
        )
        .await?,
    ))
}

async fn update_action(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateActionRequest>,
) -> AppResult<Json<ConversationAction>> {
    Ok(Json(state.conversation.update_action(&id, request).await?))
}

async fn confirm_action(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ConfirmActionRequest>,
) -> AppResult<Json<ConversationAction>> {
    Ok(Json(state.conversation.confirm_action(&id, request).await?))
}

async fn reject_action(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ConversationAction>> {
    Ok(Json(state.conversation.reject_action(&id).await?))
}

async fn upload_attachment(
    State(state): State<AppState>,
    Json(request): Json<UploadAttachmentRequest>,
) -> AppResult<Json<ConversationAttachment>> {
    Ok(Json(
        attachments::save_attachment(
            state.conversation.pool(),
            state.conversation.workspace_dir(),
            request,
        )
        .await?,
    ))
}

async fn events_ws(
    State(state): State<AppState>,
    Query(query): Query<EventQuery>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| {
        handle_event_socket(socket, state, query.after_event_id.unwrap_or(0))
    })
}

async fn handle_event_socket(mut socket: WebSocket, state: AppState, mut cursor: i64) {
    let mut receiver = state.conversation.subscribe();
    let latest_event_id = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(MAX(event_id), 0) FROM conversation_run_events",
    )
    .fetch_one(state.conversation.pool())
    .await
    .unwrap_or(0);
    if cursor > latest_event_id {
        cursor = 0;
    }
    if let Ok(events) = state.conversation.replay_events(cursor).await {
        for event in events {
            cursor = event.event_id;
            if !send_event(&mut socket, &event).await {
                return;
            }
        }
    }
    loop {
        tokio::select! {
            received = receiver.recv() => {
                match received {
                    Ok(event) => {
                        if event.event_id <= cursor {
                            continue;
                        }
                        cursor = event.event_id;
                        if !send_event(&mut socket, &event).await {
                            return;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        if let Ok(events) = state.conversation.replay_events(cursor).await {
                            for event in events {
                                cursor = event.event_id;
                                if !send_event(&mut socket, &event).await {
                                    return;
                                }
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                }
            }
            message = socket.recv() => {
                match message {
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                    Some(Ok(Message::Ping(payload))) => {
                        if socket.send(Message::Pong(payload)).await.is_err() {
                            return;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

async fn send_event(socket: &mut WebSocket, event: &RunEvent) -> bool {
    match serde_json::to_string(event) {
        Ok(serialized) => socket.send(Message::Text(serialized.into())).await.is_ok(),
        Err(error) => {
            tracing::warn!(error = %error, "failed to serialize conversation event");
            false
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(value: std::io::Error) -> Self {
        AppError::internal(value.to_string())
    }
}
