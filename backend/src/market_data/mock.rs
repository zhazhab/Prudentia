use async_trait::async_trait;

use crate::{
    market_data::{MarketDataError, MarketDataProvider, MarketQuote},
    time::now_iso,
};

pub struct MockMarketDataProvider;

#[async_trait]
impl MarketDataProvider for MockMarketDataProvider {
    async fn quote(&self, symbol: &str) -> Result<MarketQuote, MarketDataError> {
        let seed = symbol
            .bytes()
            .fold(0_u64, |acc, byte| acc.saturating_add(byte as u64));
        let price = 20.0 + (seed % 350) as f64 + ((seed % 17) as f64 / 10.0);

        Ok(MarketQuote {
            symbol: symbol.to_uppercase(),
            price,
            currency: Some("USD".to_string()),
            volume: None,
            source: "mock".to_string(),
            updated_at: now_iso(),
        })
    }
}
