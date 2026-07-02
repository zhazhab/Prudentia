use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

use crate::{
    market_data::{ExchangeRate, MarketDataError, MarketDataProvider, MarketQuote},
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

    async fn exchange_rate(
        &self,
        from_currency: &str,
        to_currency: &str,
    ) -> Result<ExchangeRate, MarketDataError> {
        let response: AlphaVantageExchangeRateResponse = self
            .client
            .get("https://www.alphavantage.co/query")
            .query(&[
                ("function", "CURRENCY_EXCHANGE_RATE"),
                ("from_currency", from_currency),
                ("to_currency", to_currency),
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

        let exchange_rate = response.realtime_currency_exchange_rate.ok_or_else(|| {
            MarketDataError::Provider("missing Realtime Currency Exchange Rate".to_string())
        })?;
        let rate = exchange_rate
            .exchange_rate
            .parse::<f64>()
            .map_err(|_| MarketDataError::Provider("invalid exchange rate".to_string()))?;

        Ok(ExchangeRate {
            from_currency: exchange_rate
                .from_currency_code
                .unwrap_or_else(|| from_currency.to_ascii_uppercase()),
            to_currency: exchange_rate
                .to_currency_code
                .unwrap_or_else(|| to_currency.to_ascii_uppercase()),
            rate,
            source: "alpha_vantage".to_string(),
            updated_at: exchange_rate.last_refreshed.unwrap_or_else(now_iso),
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

#[derive(Deserialize)]
struct AlphaVantageExchangeRateResponse {
    #[serde(rename = "Realtime Currency Exchange Rate")]
    realtime_currency_exchange_rate: Option<AlphaVantageExchangeRate>,
}

#[derive(Deserialize)]
struct AlphaVantageExchangeRate {
    #[serde(rename = "1. From_Currency Code")]
    from_currency_code: Option<String>,
    #[serde(rename = "3. To_Currency Code")]
    to_currency_code: Option<String>,
    #[serde(rename = "5. Exchange Rate")]
    exchange_rate: String,
    #[serde(rename = "6. Last Refreshed")]
    last_refreshed: Option<String>,
}
