use std::sync::Arc;

use sqlx::SqlitePool;

use crate::{ai::runtime::AiRuntime, market_data::MarketDataProvider};

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub ai: Arc<AiRuntime>,
    pub market_data: Arc<dyn MarketDataProvider>,
}
