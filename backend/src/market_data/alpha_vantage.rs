use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

use crate::{
    market_data::{MarketDataError, MarketDataProvider, MarketQuote},
    time::now_iso,
};

pub struct AlphaVantageProvider {
    client: Client,
    api_key: String,
}

impl AlphaVantageProvider {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
        }
    }
}

#[async_trait]
impl MarketDataProvider for AlphaVantageProvider {
    async fn quote(&self, symbol: &str) -> Result<MarketQuote, MarketDataError> {
        let response: AlphaVantageGlobalQuoteResponse = self
            .client
            .get("https://www.alphavantage.co/query")
            .query(&[
                ("function", "GLOBAL_QUOTE"),
                ("symbol", symbol),
                ("apikey", self.api_key.as_str()),
            ])
            .send()
            .await
            .map_err(|err| MarketDataError::Provider(err.to_string()))?
            .error_for_status()
            .map_err(|err| MarketDataError::Provider(err.to_string()))?
            .json()
            .await
            .map_err(|err| MarketDataError::Provider(err.to_string()))?;

        let quote = response
            .global_quote
            .ok_or_else(|| MarketDataError::Provider("missing Global Quote".to_string()))?;
        let price = quote
            .price
            .parse::<f64>()
            .map_err(|_| MarketDataError::Provider("invalid quote price".to_string()))?;
        let volume = quote.volume.and_then(|value| value.parse::<f64>().ok());

        Ok(MarketQuote {
            symbol: quote.symbol.unwrap_or_else(|| symbol.to_uppercase()),
            price,
            currency: Some("USD".to_string()),
            volume,
            source: "alpha_vantage".to_string(),
            updated_at: now_iso(),
        })
    }
}

#[derive(Deserialize)]
struct AlphaVantageGlobalQuoteResponse {
    #[serde(rename = "Global Quote")]
    global_quote: Option<AlphaVantageQuote>,
}

#[derive(Deserialize)]
struct AlphaVantageQuote {
    #[serde(rename = "01. symbol")]
    symbol: Option<String>,
    #[serde(rename = "05. price")]
    price: String,
    #[serde(rename = "06. volume")]
    volume: Option<String>,
}
