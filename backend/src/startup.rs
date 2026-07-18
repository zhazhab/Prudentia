use std::sync::Arc;

use axum::{routing::get, Json, Router};
use serde::Serialize;
use sqlx::SqlitePool;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::{
    ai::runtime::AiRuntime,
    ai_ws,
    config::AppConfig,
    conversation::{self, ConversationEngine},
    decision, decision_delta, investment_system,
    market_data::MarketDataProvider,
    memo, memo_thread, portfolio, profile, research, settings,
    state::AppState,
};

pub fn build_router(
    pool: SqlitePool,
    ai: Arc<AiRuntime>,
    market_data: Arc<dyn MarketDataProvider>,
) -> Router {
    let config = AppConfig::from_env();
    build_router_with_config(pool, ai, market_data, &config)
}

pub fn build_router_with_config(
    pool: SqlitePool,
    ai: Arc<AiRuntime>,
    market_data: Arc<dyn MarketDataProvider>,
    config: &AppConfig,
) -> Router {
    let conversation = ConversationEngine::new(
        pool.clone(),
        ai.clone(),
        market_data.clone(),
        conversation::research_provider_from_config(config),
        config.workspace_dir.clone(),
        config.capability_dir.clone(),
    );
    let recovery = conversation.clone();
    tokio::spawn(async move {
        if let Err(error) = recovery.recover_interrupted().await {
            tracing::warn!(error = %error, "failed to recover interrupted conversation runs");
        }
    });
    let state = AppState {
        pool,
        ai,
        market_data,
        conversation,
    };

    Router::new()
        .route("/health", get(health))
        .nest("/api/memos", memo::routes())
        .nest("/api/memo-threads", memo_thread::routes())
        .nest("/api/conversation", conversation::routes())
        .nest("/api/investment-system", investment_system::routes())
        .nest("/api/portfolio", portfolio::routes())
        .nest("/api/decisions", decision::routes())
        .nest("/api/decision-deltas", decision_delta::routes())
        .nest("/api/research", research::routes())
        .nest("/api/settings", settings::routes())
        .route("/api/ai/ws", get(ai_ws::ws_handler))
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
