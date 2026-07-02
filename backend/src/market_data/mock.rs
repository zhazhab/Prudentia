use async_trait::async_trait;

use crate::{
    market_data::{ExchangeRate, MarketDataError, MarketDataProvider, MarketQuote},
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

    async fn exchange_rate(
        &self,
        from_currency: &str,
        to_currency: &str,
    ) -> Result<ExchangeRate, MarketDataError> {
        let from = from_currency.trim().to_ascii_uppercase();
        let to = to_currency.trim().to_ascii_uppercase();
        let rate = match (from.as_str(), to.as_str()) {
            (left, right) if left == right => 1.0,
            ("USD", "CNY") => 7.2,
            ("HKD", "CNY") => 0.92,
            ("CNY", "USD") => 1.0 / 7.2,
            ("CNY", "HKD") => 1.0 / 0.92,
            _ => 1.0,
        };

        Ok(ExchangeRate {
            from_currency: from,
            to_currency: to,
            rate,
            source: "mock".to_string(),
            updated_at: now_iso(),
        })
    }
}
