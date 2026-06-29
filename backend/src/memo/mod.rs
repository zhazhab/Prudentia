use axum::{
    extract::{Path, State},
    http::HeaderMap,
    routing::{get as axum_get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::{
    ai::MemoExtraction,
    error::{AppError, AppResult},
    locale::Locale,
    state::AppState,
    time::now_iso,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memo {
    pub id: String,
    pub title: String,
    pub symbol: Option<String>,
    pub asset_type: String,
    pub thesis: String,
    pub risks: String,
    pub catalysts: String,
    pub disconfirming_evidence: String,
    pub notes: String,
    pub status: String,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateMemoRequest {
    pub title: String,
    pub symbol: Option<String>,
    pub asset_type: Option<String>,
    pub thesis: Option<String>,
    pub risks: Option<String>,
    pub catalysts: Option<String>,
    pub disconfirming_evidence: Option<String>,
    pub notes: Option<String>,
    pub status: Option<String>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateMemoRequest {
    pub title: Option<String>,
    pub symbol: Option<String>,
    pub asset_type: Option<String>,
    pub thesis: Option<String>,
    pub risks: Option<String>,
    pub catalysts: Option<String>,
    pub disconfirming_evidence: Option<String>,
    pub notes: Option<String>,
    pub status: Option<String>,
    pub tags: Option<Vec<String>>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", axum_get(list_memos).post(create_memo))
        .route("/{id}", axum_get(get_memo).patch(update_memo))
        .route("/{id}/ai/extract", post(extract_memo))
}

async fn list_memos(State(state): State<AppState>) -> AppResult<Json<Vec<Memo>>> {
    Ok(Json(list(&state.pool).await?))
}

async fn create_memo(
    State(state): State<AppState>,
    Json(request): Json<CreateMemoRequest>,
) -> AppResult<Json<Memo>> {
    Ok(Json(create(&state.pool, request).await?))
}

async fn get_memo(State(state): State<AppState>, Path(id): Path<String>) -> AppResult<Json<Memo>> {
    Ok(Json(get(&state.pool, &id).await?))
}

async fn update_memo(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateMemoRequest>,
) -> AppResult<Json<Memo>> {
    Ok(Json(update(&state.pool, &id, request).await?))
}

async fn extract_memo(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<MemoExtraction>> {
    let memo = get(&state.pool, &id).await?;
    let locale = Locale::from_headers(&headers);
    let extraction = state
        .ai
        .extract_memo(&memo, locale)
        .await
        .map_err(|err| AppError::internal(err.to_string()))?;
    Ok(Json(extraction))
}

pub async fn list(pool: &SqlitePool) -> AppResult<Vec<Memo>> {
    let rows = sqlx::query(
        r#"
        SELECT id, title, symbol, asset_type, thesis, risks, catalysts,
               disconfirming_evidence, notes, status, tags_json, created_at, updated_at
        FROM memos
        ORDER BY updated_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter().map(memo_from_row).collect()
}

pub async fn create(pool: &SqlitePool, request: CreateMemoRequest) -> AppResult<Memo> {
    if request.title.trim().is_empty() {
        return Err(AppError::bad_request("title is required"));
    }

    let now = now_iso();
    let memo = Memo {
        id: Uuid::new_v4().to_string(),
        title: request.title.trim().to_string(),
        symbol: request.symbol.and_then(non_empty_string),
        asset_type: request
            .asset_type
            .and_then(non_empty_string)
            .unwrap_or_else(|| "stock".to_string()),
        thesis: request.thesis.unwrap_or_default(),
        risks: request.risks.unwrap_or_default(),
        catalysts: request.catalysts.unwrap_or_default(),
        disconfirming_evidence: request.disconfirming_evidence.unwrap_or_default(),
        notes: request.notes.unwrap_or_default(),
        status: request.status.unwrap_or_else(|| "draft".to_string()),
        tags: request.tags.unwrap_or_default(),
        created_at: now.clone(),
        updated_at: now,
    };

    sqlx::query(
        r#"
        INSERT INTO memos (
            id, title, symbol, asset_type, thesis, risks, catalysts,
            disconfirming_evidence, notes, status, tags_json, created_at, updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&memo.id)
    .bind(&memo.title)
    .bind(&memo.symbol)
    .bind(&memo.asset_type)
    .bind(&memo.thesis)
    .bind(&memo.risks)
    .bind(&memo.catalysts)
    .bind(&memo.disconfirming_evidence)
    .bind(&memo.notes)
    .bind(&memo.status)
    .bind(serde_json::to_string(&memo.tags)?)
    .bind(&memo.created_at)
    .bind(&memo.updated_at)
    .execute(pool)
    .await?;

    Ok(memo)
}

pub async fn get(pool: &SqlitePool, id: &str) -> AppResult<Memo> {
    let row = sqlx::query(
        r#"
        SELECT id, title, symbol, asset_type, thesis, risks, catalysts,
               disconfirming_evidence, notes, status, tags_json, created_at, updated_at
        FROM memos
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::not_found("memo not found"))?;

    memo_from_row(row)
}

pub async fn update(pool: &SqlitePool, id: &str, request: UpdateMemoRequest) -> AppResult<Memo> {
    let mut memo = get(pool, id).await?;

    if let Some(title) = request.title {
        if title.trim().is_empty() {
            return Err(AppError::bad_request("title cannot be empty"));
        }
        memo.title = title.trim().to_string();
    }

    if let Some(symbol) = request.symbol {
        memo.symbol = non_empty_string(symbol);
    }
    if let Some(asset_type) = request.asset_type.and_then(non_empty_string) {
        memo.asset_type = asset_type;
    }
    if let Some(thesis) = request.thesis {
        memo.thesis = thesis;
    }
    if let Some(risks) = request.risks {
        memo.risks = risks;
    }
    if let Some(catalysts) = request.catalysts {
        memo.catalysts = catalysts;
    }
    if let Some(disconfirming_evidence) = request.disconfirming_evidence {
        memo.disconfirming_evidence = disconfirming_evidence;
    }
    if let Some(notes) = request.notes {
        memo.notes = notes;
    }
    if let Some(status) = request.status.and_then(non_empty_string) {
        memo.status = status;
    }
    if let Some(tags) = request.tags {
        memo.tags = tags;
    }
    memo.updated_at = now_iso();

    sqlx::query(
        r#"
        UPDATE memos
        SET title = ?, symbol = ?, asset_type = ?, thesis = ?, risks = ?, catalysts = ?,
            disconfirming_evidence = ?, notes = ?, status = ?, tags_json = ?, updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(&memo.title)
    .bind(&memo.symbol)
    .bind(&memo.asset_type)
    .bind(&memo.thesis)
    .bind(&memo.risks)
    .bind(&memo.catalysts)
    .bind(&memo.disconfirming_evidence)
    .bind(&memo.notes)
    .bind(&memo.status)
    .bind(serde_json::to_string(&memo.tags)?)
    .bind(&memo.updated_at)
    .bind(&memo.id)
    .execute(pool)
    .await?;

    Ok(memo)
}

fn memo_from_row(row: sqlx::sqlite::SqliteRow) -> AppResult<Memo> {
    let tags_json: String = row.try_get("tags_json")?;
    let tags = serde_json::from_str(&tags_json).unwrap_or_default();

    Ok(Memo {
        id: row.try_get("id")?,
        title: row.try_get("title")?,
        symbol: row.try_get("symbol")?,
        asset_type: row.try_get("asset_type")?,
        thesis: row.try_get("thesis")?,
        risks: row.try_get("risks")?,
        catalysts: row.try_get("catalysts")?,
        disconfirming_evidence: row.try_get("disconfirming_evidence")?,
        notes: row.try_get("notes")?,
        status: row.try_get("status")?,
        tags,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn non_empty_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
