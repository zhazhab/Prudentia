use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    locale::Locale,
    state::AppState,
    time::now_iso,
};

const DEFAULT_THREAD_LIMIT: i64 = 12;
const DEFAULT_MESSAGE_LIMIT: i64 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoThreadMessageRole {
    User,
    Assistant,
    System,
}

impl MemoThreadMessageRole {
    fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::System => "system",
        }
    }

    fn parse(value: &str) -> AppResult<Self> {
        match value {
            "user" => Ok(Self::User),
            "assistant" => Ok(Self::Assistant),
            "system" => Ok(Self::System),
            _ => Err(AppError::internal("invalid memo thread message role")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoThreadMessageStatus {
    Pending,
    Streaming,
    Completed,
    Canceled,
    Failed,
}

impl MemoThreadMessageStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Streaming => "streaming",
            Self::Completed => "completed",
            Self::Canceled => "canceled",
            Self::Failed => "failed",
        }
    }

    fn parse(value: &str) -> AppResult<Self> {
        match value {
            "pending" => Ok(Self::Pending),
            "streaming" => Ok(Self::Streaming),
            "completed" => Ok(Self::Completed),
            "canceled" => Ok(Self::Canceled),
            "failed" => Ok(Self::Failed),
            _ => Err(AppError::internal("invalid memo thread message status")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoThreadSummary {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub status: String,
    pub linked_symbols: Vec<String>,
    pub tags: Vec<String>,
    pub archived_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_message_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoThreadMessage {
    pub id: String,
    pub thread_id: String,
    pub role: MemoThreadMessageRole,
    pub content: String,
    pub status: MemoThreadMessageStatus,
    pub request_id: Option<String>,
    pub duration_ms: Option<i64>,
    pub artifacts: Vec<Value>,
    pub sources: Vec<Value>,
    pub used_context: Vec<Value>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoThreadDetail {
    pub thread: MemoThreadSummary,
    pub messages: Vec<MemoThreadMessage>,
    pub has_more: bool,
}

#[derive(Debug, Clone)]
pub struct CreateMemoThreadMessageRequest {
    pub thread_id: Option<String>,
    pub client_thread_id: Option<String>,
    pub content: String,
    pub locale: Locale,
}

#[derive(Debug, Clone)]
pub struct AppendAssistantMessageRequest {
    pub request_id: Option<String>,
    pub content: String,
    pub status: MemoThreadMessageStatus,
    pub duration_ms: Option<i64>,
    pub artifacts: Vec<Value>,
    pub sources: Vec<Value>,
    pub used_context: Vec<Value>,
}

#[derive(Debug, Deserialize)]
struct ListThreadsQuery {
    limit: Option<i64>,
    include_archived: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ThreadDetailQuery {
    message_limit: Option<i64>,
    before_message_id: Option<String>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_threads))
        .route("/{id}", get(get_thread).delete(delete_thread))
        .route("/{id}/archive", post(archive_thread))
}

async fn list_threads(
    State(state): State<AppState>,
    Query(query): Query<ListThreadsQuery>,
) -> AppResult<Json<Vec<MemoThreadSummary>>> {
    Ok(Json(
        list(
            &state.pool,
            query.limit.unwrap_or(DEFAULT_THREAD_LIMIT),
            query.include_archived.unwrap_or(false),
        )
        .await?,
    ))
}

async fn get_thread(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<ThreadDetailQuery>,
) -> AppResult<Json<MemoThreadDetail>> {
    Ok(Json(
        get_detail(
            &state.pool,
            &id,
            query.message_limit.unwrap_or(DEFAULT_MESSAGE_LIMIT),
            query.before_message_id.as_deref(),
        )
        .await?,
    ))
}

async fn archive_thread(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<MemoThreadSummary>> {
    Ok(Json(archive(&state.pool, &id).await?))
}

async fn delete_thread(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<MemoThreadSummary>> {
    Ok(Json(soft_delete(&state.pool, &id).await?))
}

pub async fn create_thread_with_user_message(
    pool: &SqlitePool,
    request: CreateMemoThreadMessageRequest,
) -> AppResult<MemoThreadSummary> {
    let content = clean_content(&request.content)?;
    let now = now_iso();
    let thread_id = request
        .thread_id
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let existing = find_thread(pool, &thread_id).await?;
    let thread = if let Some(thread) = existing {
        thread
    } else {
        let title = title_from_content(&content, request.locale);
        sqlx::query(
            r#"
            INSERT INTO memo_threads (
                id, title, summary, status, linked_symbols_json, tags_json,
                archived_at, deleted_at, client_thread_id, created_at, updated_at, last_message_at
            )
            VALUES (?, ?, ?, 'active', '[]', '[]', NULL, NULL, ?, ?, ?, ?)
            "#,
        )
        .bind(&thread_id)
        .bind(&title)
        .bind("")
        .bind(&request.client_thread_id)
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await?;

        find_thread(pool, &thread_id)
            .await?
            .ok_or_else(|| AppError::internal("failed to create memo thread"))?
    };

    insert_message(
        pool,
        InsertMessageRequest {
            thread_id: &thread.id,
            role: MemoThreadMessageRole::User,
            content: &content,
            status: MemoThreadMessageStatus::Completed,
            request_id: None,
            duration_ms: None,
            artifacts: Vec::new(),
            sources: Vec::new(),
            used_context: Vec::new(),
        },
    )
    .await?;

    touch_thread(pool, &thread.id).await?;
    find_thread(pool, &thread.id)
        .await?
        .ok_or_else(|| AppError::internal("memo thread disappeared"))
}

pub async fn append_assistant_message(
    pool: &SqlitePool,
    thread_id: &str,
    request: AppendAssistantMessageRequest,
) -> AppResult<MemoThreadMessage> {
    let message = insert_message(
        pool,
        InsertMessageRequest {
            thread_id,
            role: MemoThreadMessageRole::Assistant,
            content: &request.content,
            status: request.status,
            request_id: request.request_id.as_deref(),
            duration_ms: request.duration_ms,
            artifacts: request.artifacts,
            sources: request.sources,
            used_context: request.used_context,
        },
    )
    .await?;
    touch_thread(pool, thread_id).await?;
    Ok(message)
}

pub async fn list(
    pool: &SqlitePool,
    limit: i64,
    include_archived: bool,
) -> AppResult<Vec<MemoThreadSummary>> {
    let limit = limit.clamp(1, 50);
    let rows = if include_archived {
        sqlx::query(
            r#"
            SELECT id, title, summary, status, linked_symbols_json, tags_json,
                   archived_at, created_at, updated_at, last_message_at
            FROM memo_threads
            WHERE deleted_at IS NULL
            ORDER BY last_message_at DESC, updated_at DESC
            LIMIT ?
            "#,
        )
        .bind(limit)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            r#"
            SELECT id, title, summary, status, linked_symbols_json, tags_json,
                   archived_at, created_at, updated_at, last_message_at
            FROM memo_threads
            WHERE deleted_at IS NULL AND archived_at IS NULL
            ORDER BY last_message_at DESC, updated_at DESC
            LIMIT ?
            "#,
        )
        .bind(limit)
        .fetch_all(pool)
        .await?
    };

    rows.into_iter().map(thread_from_row).collect()
}

pub async fn get_detail(
    pool: &SqlitePool,
    thread_id: &str,
    message_limit: i64,
    before_message_id: Option<&str>,
) -> AppResult<MemoThreadDetail> {
    let thread = find_thread(pool, thread_id)
        .await?
        .ok_or_else(|| AppError::not_found("memo thread not found"))?;

    let limit = message_limit.clamp(1, 100);
    let rows = if let Some(before_message_id) = before_message_id {
        let before_created_at = message_created_at(pool, thread_id, before_message_id).await?;
        sqlx::query(
            r#"
            SELECT id, thread_id, role, content, status, request_id, duration_ms,
                   artifacts_json, sources_json, used_context_json, created_at, updated_at
            FROM memo_thread_messages
            WHERE thread_id = ? AND created_at < ?
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(thread_id)
        .bind(before_created_at)
        .bind(limit + 1)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            r#"
            SELECT id, thread_id, role, content, status, request_id, duration_ms,
                   artifacts_json, sources_json, used_context_json, created_at, updated_at
            FROM memo_thread_messages
            WHERE thread_id = ?
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(thread_id)
        .bind(limit + 1)
        .fetch_all(pool)
        .await?
    };

    let has_more = rows.len() as i64 > limit;
    let mut messages = rows
        .into_iter()
        .take(limit as usize)
        .map(message_from_row)
        .collect::<AppResult<Vec<_>>>()?;
    messages.reverse();

    Ok(MemoThreadDetail {
        thread,
        messages,
        has_more,
    })
}

pub async fn archive(pool: &SqlitePool, thread_id: &str) -> AppResult<MemoThreadSummary> {
    let now = now_iso();
    let result = sqlx::query(
        r#"
        UPDATE memo_threads
        SET archived_at = ?, status = 'archived', updated_at = ?
        WHERE id = ? AND deleted_at IS NULL
        "#,
    )
    .bind(&now)
    .bind(&now)
    .bind(thread_id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::not_found("memo thread not found"));
    }

    find_thread(pool, thread_id)
        .await?
        .ok_or_else(|| AppError::not_found("memo thread not found"))
}

pub async fn soft_delete(pool: &SqlitePool, thread_id: &str) -> AppResult<MemoThreadSummary> {
    let thread = find_thread(pool, thread_id)
        .await?
        .ok_or_else(|| AppError::not_found("memo thread not found"))?;
    let now = now_iso();
    sqlx::query(
        r#"
        UPDATE memo_threads
        SET deleted_at = ?, status = 'deleted', updated_at = ?
        WHERE id = ? AND deleted_at IS NULL
        "#,
    )
    .bind(&now)
    .bind(&now)
    .bind(thread_id)
    .execute(pool)
    .await?;
    Ok(thread)
}

async fn find_thread(pool: &SqlitePool, thread_id: &str) -> AppResult<Option<MemoThreadSummary>> {
    let row = sqlx::query(
        r#"
        SELECT id, title, summary, status, linked_symbols_json, tags_json,
               archived_at, created_at, updated_at, last_message_at
        FROM memo_threads
        WHERE id = ? AND deleted_at IS NULL
        "#,
    )
    .bind(thread_id)
    .fetch_optional(pool)
    .await?;

    row.map(thread_from_row).transpose()
}

struct InsertMessageRequest<'a> {
    thread_id: &'a str,
    role: MemoThreadMessageRole,
    content: &'a str,
    status: MemoThreadMessageStatus,
    request_id: Option<&'a str>,
    duration_ms: Option<i64>,
    artifacts: Vec<Value>,
    sources: Vec<Value>,
    used_context: Vec<Value>,
}

async fn insert_message(
    pool: &SqlitePool,
    request: InsertMessageRequest<'_>,
) -> AppResult<MemoThreadMessage> {
    let now = now_iso();
    let message = MemoThreadMessage {
        id: Uuid::new_v4().to_string(),
        thread_id: request.thread_id.to_string(),
        role: request.role,
        content: request.content.to_string(),
        status: request.status,
        request_id: request.request_id.map(ToOwned::to_owned),
        duration_ms: request.duration_ms,
        artifacts: request.artifacts,
        sources: request.sources,
        used_context: request.used_context,
        created_at: now.clone(),
        updated_at: now,
    };

    sqlx::query(
        r#"
        INSERT INTO memo_thread_messages (
            id, thread_id, role, content, status, request_id, duration_ms,
            artifacts_json, sources_json, used_context_json, created_at, updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&message.id)
    .bind(&message.thread_id)
    .bind(message.role.as_str())
    .bind(&message.content)
    .bind(message.status.as_str())
    .bind(&message.request_id)
    .bind(message.duration_ms)
    .bind(serde_json::to_string(&message.artifacts)?)
    .bind(serde_json::to_string(&message.sources)?)
    .bind(serde_json::to_string(&message.used_context)?)
    .bind(&message.created_at)
    .bind(&message.updated_at)
    .execute(pool)
    .await?;

    Ok(message)
}

async fn touch_thread(pool: &SqlitePool, thread_id: &str) -> AppResult<()> {
    let now = now_iso();
    sqlx::query(
        r#"
        UPDATE memo_threads
        SET updated_at = ?, last_message_at = ?
        WHERE id = ?
        "#,
    )
    .bind(&now)
    .bind(&now)
    .bind(thread_id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn message_created_at(
    pool: &SqlitePool,
    thread_id: &str,
    message_id: &str,
) -> AppResult<String> {
    sqlx::query(
        r#"
        SELECT created_at
        FROM memo_thread_messages
        WHERE thread_id = ? AND id = ?
        "#,
    )
    .bind(thread_id)
    .bind(message_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::not_found("memo thread message not found"))?
    .try_get("created_at")
    .map_err(AppError::from)
}

fn thread_from_row(row: sqlx::sqlite::SqliteRow) -> AppResult<MemoThreadSummary> {
    Ok(MemoThreadSummary {
        id: row.try_get("id")?,
        title: row.try_get("title")?,
        summary: row.try_get("summary")?,
        status: row.try_get("status")?,
        linked_symbols: serde_json::from_str(&row.try_get::<String, _>("linked_symbols_json")?)?,
        tags: serde_json::from_str(&row.try_get::<String, _>("tags_json")?)?,
        archived_at: row.try_get("archived_at")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        last_message_at: row.try_get("last_message_at")?,
    })
}

fn message_from_row(row: sqlx::sqlite::SqliteRow) -> AppResult<MemoThreadMessage> {
    let role = MemoThreadMessageRole::parse(&row.try_get::<String, _>("role")?)?;
    let status = MemoThreadMessageStatus::parse(&row.try_get::<String, _>("status")?)?;
    Ok(MemoThreadMessage {
        id: row.try_get("id")?,
        thread_id: row.try_get("thread_id")?,
        role,
        content: row.try_get("content")?,
        status,
        request_id: row.try_get("request_id")?,
        duration_ms: row.try_get("duration_ms")?,
        artifacts: serde_json::from_str(&row.try_get::<String, _>("artifacts_json")?)?,
        sources: serde_json::from_str(&row.try_get::<String, _>("sources_json")?)?,
        used_context: serde_json::from_str(&row.try_get::<String, _>("used_context_json")?)?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn clean_content(content: &str) -> AppResult<String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err(AppError::bad_request("message content is required"));
    }
    Ok(trimmed.to_string())
}

fn title_from_content(content: &str, locale: Locale) -> String {
    let max_chars = 40;
    let mut title = content.chars().take(max_chars).collect::<String>();
    if content.chars().count() > max_chars {
        title.push('…');
    }
    if title.trim().is_empty() {
        if locale.is_zh() {
            "未命名主题".to_string()
        } else {
            "Untitled thread".to_string()
        }
    } else {
        title
    }
}
