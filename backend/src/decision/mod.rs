use axum::{
    extract::{Path, State},
    routing::get as axum_get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{
    decision_delta::{self, DecisionDeltaInput},
    error::{AppError, AppResult},
    state::AppState,
    time::now_iso,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub id: String,
    pub memo_id: Option<String>,
    pub symbol: Option<String>,
    pub action: String,
    pub rationale: String,
    pub confidence: f64,
    pub expected_outcome: String,
    pub review_date: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateDecisionRequest {
    pub memo_id: Option<String>,
    pub symbol: Option<String>,
    pub action: String,
    pub rationale: String,
    pub confidence: f64,
    pub expected_outcome: String,
    pub review_date: Option<String>,
    pub decision_date: Option<String>,
    pub quantity: Option<f64>,
    pub notional: Option<f64>,
    pub price: Option<f64>,
    pub currency: Option<String>,
    pub baseline_type: Option<String>,
    pub hypothetical_notional: Option<f64>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            axum_get(list_decisions_handler).post(create_decision_handler),
        )
        .route("/{id}", axum_get(get_decision_handler))
}

async fn list_decisions_handler(State(state): State<AppState>) -> AppResult<Json<Vec<Decision>>> {
    Ok(Json(list(&state.pool).await?))
}

async fn get_decision_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Decision>> {
    Ok(Json(get(&state.pool, &id).await?))
}

async fn create_decision_handler(
    State(state): State<AppState>,
    Json(request): Json<CreateDecisionRequest>,
) -> AppResult<Json<Decision>> {
    Ok(Json(create(&state.pool, request).await?))
}

pub async fn create(pool: &SqlitePool, request: CreateDecisionRequest) -> AppResult<Decision> {
    if request.action.trim().is_empty() {
        return Err(AppError::bad_request("action is required"));
    }
    if request.rationale.trim().is_empty() {
        return Err(AppError::bad_request("rationale is required"));
    }
    if !(0.0..=1.0).contains(&request.confidence) {
        return Err(AppError::bad_request("confidence must be between 0 and 1"));
    }

    let quantification = DecisionDeltaInput {
        action: request.action.trim().to_string(),
        symbol: request.symbol.clone(),
        quantity: request.quantity,
        notional: request.notional,
        price: request.price,
        currency: request.currency.clone(),
        baseline_type: request.baseline_type.clone(),
        hypothetical_notional: request.hypothetical_notional,
    };

    let decision = Decision {
        id: Uuid::new_v4().to_string(),
        memo_id: request.memo_id,
        symbol: request.symbol.map(|value| value.trim().to_uppercase()),
        action: request.action.trim().to_string(),
        rationale: request.rationale,
        confidence: request.confidence,
        expected_outcome: request.expected_outcome,
        review_date: request.review_date,
        created_at: now_iso(),
    };

    sqlx::query(
        r#"
        INSERT INTO decisions (
            id, memo_id, symbol, action, rationale, confidence,
            expected_outcome, review_date, created_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&decision.id)
    .bind(&decision.memo_id)
    .bind(&decision.symbol)
    .bind(&decision.action)
    .bind(&decision.rationale)
    .bind(decision.confidence)
    .bind(&decision.expected_outcome)
    .bind(&decision.review_date)
    .bind(&decision.created_at)
    .execute(pool)
    .await?;

    decision_delta::create_legs_for_decision(pool, &decision.id, quantification).await?;

    Ok(decision)
}

pub async fn list(pool: &SqlitePool) -> AppResult<Vec<Decision>> {
    let rows = sqlx::query(
        r#"
        SELECT id, memo_id, symbol, action, rationale, confidence,
               expected_outcome, review_date, created_at
        FROM decisions
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter().map(decision_from_row).collect()
}

pub async fn get(pool: &SqlitePool, id: &str) -> AppResult<Decision> {
    let row = sqlx::query(
        r#"
        SELECT id, memo_id, symbol, action, rationale, confidence,
               expected_outcome, review_date, created_at
        FROM decisions
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::not_found("decision not found"))?;

    decision_from_row(row)
}

fn decision_from_row(row: sqlx::sqlite::SqliteRow) -> AppResult<Decision> {
    use sqlx::Row;

    Ok(Decision {
        id: row.try_get("id")?,
        memo_id: row.try_get("memo_id")?,
        symbol: row.try_get("symbol")?,
        action: row.try_get("action")?,
        rationale: row.try_get("rationale")?,
        confidence: row.try_get("confidence")?,
        expected_outcome: row.try_get("expected_outcome")?,
        review_date: row.try_get("review_date")?,
        created_at: row.try_get("created_at")?,
    })
}
