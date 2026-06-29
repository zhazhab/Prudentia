use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::config::AppConfig;

pub mod alpha_vantage;
pub mod mock;

#[async_trait]
pub trait MarketDataProvider: Send + Sync {
    async fn quote(&self, symbol: &str) -> Result<MarketQuote, MarketDataError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketQuote {
    pub symbol: String,
    pub price: f64,
    pub currency: Option<String>,
    pub volume: Option<f64>,
    pub source: String,
    pub updated_at: String,
}

#[derive(Debug, Error)]
pub enum MarketDataError {
    #[error("{0}")]
    Provider(String),
}

pub fn provider_from_config(config: &AppConfig) -> Arc<dyn MarketDataProvider> {
    if config
        .market_data_provider
        .eq_ignore_ascii_case("alpha_vantage")
    {
        if let Some(api_key) = &config.alpha_vantage_api_key {
            return Arc::new(alpha_vantage::AlphaVantageProvider::new(api_key.clone()));
        }

        tracing::warn!(
            "MARKET_DATA_PROVIDER=alpha_vantage was set without ALPHA_VANTAGE_API_KEY; falling back to mock quotes"
        );
    }

    Arc::new(mock::MockMarketDataProvider)
}
