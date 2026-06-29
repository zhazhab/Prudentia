use axum::{extract::State, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{
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
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/", post(create_decision_handler))
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

    Ok(decision)
}
