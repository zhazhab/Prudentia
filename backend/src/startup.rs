use std::sync::Arc;

use axum::{routing::get, Json, Router};
use serde::Serialize;
use sqlx::SqlitePool;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::{
    ai::runtime::AiRuntime, decision, investment_system, market_data::MarketDataProvider, memo,
    portfolio, profile, settings, state::AppState,
};

pub fn build_router(
    pool: SqlitePool,
    ai: Arc<AiRuntime>,
    market_data: Arc<dyn MarketDataProvider>,
) -> Router {
    let state = AppState {
        pool,
        ai,
        market_data,
    };

    Router::new()
        .route("/health", get(health))
        .nest("/api/memos", memo::routes())
        .nest("/api/investment-system", investment_system::routes())
        .nest("/api/portfolio", portfolio::routes())
        .nest("/api/decisions", decision::routes())
        .nest("/api/settings", settings::routes())
        .route("/api/profile", get(profile::get_profile))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}
