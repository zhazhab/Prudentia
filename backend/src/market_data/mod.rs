use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::config::AppConfig;

pub mod alpha_vantage;
pub mod longbridge;
pub mod mock;
mod rate_limit;
pub mod tencent;
pub mod yahoo;

pub use rate_limit::RateLimitedMarketDataProvider;

#[async_trait]
pub trait MarketDataProvider: Send + Sync {
    fn supports_batch_quotes(&self) -> bool {
        false
    }

    async fn quote(&self, symbol: &str) -> Result<MarketQuote, MarketDataError>;
    async fn quotes(&self, symbols: &[String]) -> Vec<Result<MarketQuote, MarketDataError>> {
        let mut quotes = Vec::with_capacity(symbols.len());
        for symbol in symbols {
            quotes.push(self.quote(symbol).await);
        }
        quotes
    }

    async fn exchange_rate(
        &self,
        from_currency: &str,
        to_currency: &str,
    ) -> Result<ExchangeRate, MarketDataError>;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeRate {
    pub from_currency: String,
    pub to_currency: String,
    pub rate: f64,
    pub source: String,
    pub updated_at: String,
}

#[derive(Debug, Error)]
pub enum MarketDataError {
    #[error("{0}")]
    Provider(String),
    #[error("{0}")]
    RateLimited(String),
}

impl MarketDataError {
    pub fn is_rate_limited(&self) -> bool {
        match self {
            Self::RateLimited(_) => true,
            Self::Provider(message) => {
                let normalized = message.to_ascii_lowercase();
                normalized.contains("429")
                    || normalized.contains("too many requests")
                    || normalized.contains("rate limit")
                    || normalized.contains("rate-limit")
                    || normalized.contains("频控")
            }
        }
    }
}

pub fn provider_from_config(config: &AppConfig) -> Arc<dyn MarketDataProvider> {
    let mut providers = Vec::new();
    for key in market_data_provider_keys(&config.market_data_provider) {
        match key.as_str() {
            "alpha_vantage" | "alphavantage" => {
                if let Some(api_key) = &config.alpha_vantage_api_key {
                    providers.push(rate_limited_provider(
                        "alpha_vantage",
                        Arc::new(alpha_vantage::AlphaVantageProvider::new(api_key.clone())),
                    ));
                } else {
                    tracing::warn!(
                        "MARKET_DATA_PROVIDER=alpha_vantage was set without ALPHA_VANTAGE_API_KEY; provider skipped"
                    );
                }
            }
            "yahoo" | "yahoo_finance" => {
                providers.push(rate_limited_provider(
                    "yahoo",
                    Arc::new(yahoo::YahooMarketDataProvider::new()),
                ));
            }
            "tencent" | "qq" => {
                providers.push(rate_limited_provider(
                    "tencent",
                    Arc::new(tencent::TencentMarketDataProvider::new()),
                ));
            }
            "longbridge" | "longport" => match longbridge::LongbridgeMarketDataProvider::new() {
                Ok(provider) => {
                    providers.push(rate_limited_provider("longbridge", Arc::new(provider)))
                }
                Err(error) => tracing::warn!(
                    error = ?error,
                    "MARKET_DATA_PROVIDER=longbridge was set without usable Longbridge credentials; provider skipped"
                ),
            },
            "mock" => providers.push(Arc::new(mock::MockMarketDataProvider)),
            other => {
                tracing::warn!(
                    provider = other,
                    "unsupported MARKET_DATA_PROVIDER entry; provider skipped"
                );
            }
        }
    }

    match providers.len() {
        0 => Arc::new(mock::MockMarketDataProvider),
        1 => providers.remove(0),
        _ => Arc::new(FallbackMarketDataProvider { providers }),
    }
}

fn rate_limited_provider(
    provider_key: &str,
    provider: Arc<dyn MarketDataProvider>,
) -> Arc<dyn MarketDataProvider> {
    Arc::new(rate_limit::RateLimitedMarketDataProvider::new(
        provider_key,
        provider,
        rate_limit::provider_rate_limits(provider_key),
    ))
}

pub fn market_data_provider_keys(value: &str) -> Vec<String> {
    let keys = value
        .split(',')
        .map(|entry| entry.trim().to_ascii_lowercase())
        .filter(|entry| !entry.is_empty())
        .collect::<Vec<_>>();
    if keys.is_empty() {
        vec!["mock".to_string()]
    } else {
        keys
    }
}

struct FallbackMarketDataProvider {
    providers: Vec<Arc<dyn MarketDataProvider>>,
}

#[async_trait]
impl MarketDataProvider for FallbackMarketDataProvider {
    fn supports_batch_quotes(&self) -> bool {
        true
    }

    async fn quote(&self, symbol: &str) -> Result<MarketQuote, MarketDataError> {
        self.quotes(&[symbol.to_string()])
            .await
            .into_iter()
            .next()
            .unwrap_or_else(|| {
                Err(MarketDataError::Provider(format!(
                    "{symbol}: missing market data provider result"
                )))
            })
    }

    async fn quotes(&self, symbols: &[String]) -> Vec<Result<MarketQuote, MarketDataError>> {
        let mut results = (0..symbols.len()).map(|_| None).collect::<Vec<_>>();
        let mut pending = (0..symbols.len()).collect::<Vec<_>>();
        let mut errors = (0..symbols.len()).map(|_| Vec::new()).collect::<Vec<_>>();

        for provider in &self.providers {
            if pending.is_empty() {
                break;
            }

            let pending_symbols = pending
                .iter()
                .map(|index| symbols[*index].clone())
                .collect::<Vec<_>>();
            let mut provider_results = provider.quotes(&pending_symbols).await.into_iter();
            let current_pending = std::mem::take(&mut pending);

            for index in current_pending {
                match provider_results.next() {
                    Some(Ok(quote)) => results[index] = Some(Ok(quote)),
                    Some(Err(error)) => {
                        errors[index].push(error.to_string());
                        pending.push(index);
                    }
                    None => {
                        errors[index].push("missing batch quote result".to_string());
                        pending.push(index);
                    }
                }
            }
        }

        results
            .into_iter()
            .enumerate()
            .map(|(index, result)| {
                result.unwrap_or_else(|| {
                    Err(MarketDataError::Provider(format!(
                        "{}: all market data providers failed: {}",
                        symbols[index],
                        errors[index].join("; ")
                    )))
                })
            })
            .collect()
    }

    async fn exchange_rate(
        &self,
        from_currency: &str,
        to_currency: &str,
    ) -> Result<ExchangeRate, MarketDataError> {
        let mut errors = Vec::new();
        for provider in &self.providers {
            match provider.exchange_rate(from_currency, to_currency).await {
                Ok(rate) => return Ok(rate),
                Err(error) => errors.push(error.to_string()),
            }
        }
        Err(MarketDataError::Provider(format!(
            "{from_currency}/{to_currency}: all market data providers failed: {}",
            errors.join("; ")
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        longbridge::longbridge_symbol,
        market_data_provider_keys,
        tencent::parse_tencent_quote_responses,
        tencent::{parse_tencent_quote_response, tencent_symbol},
        yahoo::parse_yahoo_chart_response,
        ExchangeRate, MarketDataError, MarketDataProvider, MarketQuote,
        RateLimitedMarketDataProvider,
    };
    use async_trait::async_trait;
    use std::{
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        time::Duration,
    };

    struct RateLimitedOnceProvider {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl MarketDataProvider for RateLimitedOnceProvider {
        async fn quote(&self, _symbol: &str) -> Result<MarketQuote, MarketDataError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Err(MarketDataError::Provider(
                "HTTP status client error (429 Too Many Requests)".to_string(),
            ))
        }

        async fn exchange_rate(
            &self,
            _from_currency: &str,
            _to_currency: &str,
        ) -> Result<ExchangeRate, MarketDataError> {
            Err(MarketDataError::Provider(
                "HTTP status client error (429 Too Many Requests)".to_string(),
            ))
        }
    }

    #[test]
    fn market_data_provider_keys_parse_fallback_chain() {
        assert_eq!(
            market_data_provider_keys(" yahoo, tencent , longbridge "),
            vec!["yahoo", "tencent", "longbridge"]
        );
        assert_eq!(market_data_provider_keys(""), vec!["mock"]);
    }

    #[tokio::test]
    async fn rate_limited_provider_cools_down_after_provider_throttle() {
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = RateLimitedMarketDataProvider::with_limits(
            "test",
            Arc::new(RateLimitedOnceProvider {
                calls: calls.clone(),
            }),
            Duration::ZERO,
            Duration::from_secs(60),
        );

        let first = provider
            .quote("PDD")
            .await
            .expect_err("first call is rate limited");
        assert!(first.to_string().contains("429"));

        let second = provider
            .quote("PDD")
            .await
            .expect_err("second call is blocked by cooldown");
        assert!(second.to_string().contains("cooling down"));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn longbridge_symbol_maps_internal_symbols_to_code_market_format() {
        assert_eq!(longbridge_symbol("0700.HK"), "700.HK");
        assert_eq!(longbridge_symbol("0883.HK"), "883.HK");
        assert_eq!(longbridge_symbol("600036.SS"), "600036.SH");
        assert_eq!(longbridge_symbol("000001.SZ"), "000001.SZ");
        assert_eq!(longbridge_symbol("PDD"), "PDD.US");
    }

    #[test]
    fn yahoo_chart_response_extracts_quote_and_currency() {
        let response = r#"{
          "chart": {
            "result": [{
              "meta": {
                "currency": "HKD",
                "symbol": "0700.HK",
                "regularMarketTime": 1783439980,
                "regularMarketPrice": 461.2
              },
              "indicators": {
                "quote": [{
                  "volume": [55418434]
                }]
              }
            }],
            "error": null
          }
        }"#;

        let quote = parse_yahoo_chart_response("0700.HK", response).expect("quote");

        assert_eq!(quote.symbol, "0700.HK");
        assert_eq!(quote.currency.as_deref(), Some("HKD"));
        assert_eq!(quote.price, 461.2);
        assert_eq!(quote.volume, Some(55_418_434.0));
        assert_eq!(quote.source, "yahoo");
    }

    #[test]
    fn tencent_symbol_maps_internal_symbols_to_quote_codes() {
        assert_eq!(tencent_symbol("0700.HK").as_deref(), Some("s_hk00700"));
        assert_eq!(tencent_symbol("0883.HK").as_deref(), Some("s_hk00883"));
        assert_eq!(tencent_symbol("600036.SS").as_deref(), Some("s_sh600036"));
        assert_eq!(tencent_symbol("000001.SZ").as_deref(), Some("s_sz000001"));
        assert_eq!(tencent_symbol("000001.SS").as_deref(), Some("sh000001"));
        assert_eq!(tencent_symbol("510300").as_deref(), Some("s_sh510300"));
        assert_eq!(tencent_symbol("159201").as_deref(), Some("s_sz159201"));
        assert_eq!(tencent_symbol("PDD").as_deref(), Some("usPDD"));
    }

    #[test]
    fn tencent_response_extracts_simple_quote_price() {
        let response = r#"v_s_hk00700="100~腾讯控股~00700~461.200~9.200~2.04~55418434.0~25937523114.184~~41934.0138";"#;

        let quote = parse_tencent_quote_response("0700.HK", response).expect("quote");

        assert_eq!(quote.symbol, "0700.HK");
        assert_eq!(quote.currency.as_deref(), Some("HKD"));
        assert_eq!(quote.price, 461.2);
        assert_eq!(quote.volume, Some(55_418_434.0));
        assert_eq!(quote.source, "tencent");
    }

    #[test]
    fn tencent_response_extracts_index_quote_price() {
        let response = r#"v_sh000001="1~上证指数~000001~4036.59~3970.88~3977.55~553063980~0~0~0.00~0~0.00~0~0.00~0~0.00~0~0.00~0~0.00~0~0.00~0~0.00~0~0.00~0~~20260709161402~65.71~1.65~4040.54~3938.88~4036.59/553063980/1364176552621~553063980~136417655~1.15~17.88~~4040.54~3938.88~2.56~625739.20~677512.85~0.00~-1~-1~0.97~0~3981.28~~~~~~136417655.2621~0.0000~0~ ~ZS~1.71~0.19~~~~4258.86~3483.38~-2.03~1.09~1.26~4826883246600~~-2.15~3.78~4826883246600~~~15.56~0.07~~CNY~0~~0.00~0~";"#;

        let quote = parse_tencent_quote_response("000001.SS", response).expect("quote");

        assert_eq!(quote.symbol, "000001.SS");
        assert_eq!(quote.currency.as_deref(), Some("CNY"));
        assert_eq!(quote.price, 4036.59);
        assert_eq!(quote.source, "tencent");
    }

    #[test]
    fn tencent_response_extracts_batch_quotes_in_request_order() {
        let response = r#"v_s_hk00700="100~腾讯控股~00700~461.200~9.200~2.04~55418434.0~25937523114.184~~41934.0138";
v_s_sh600036="1~招商银行~600036~37.550~0.100~0.27~123456~0~0~0~0~0~0~0~0~0~0~0~0~0~0~0~0~0~0~0~0~0~0~~2026-07-07 15:00:00~0.10~0.27~37.80~37.10~CNY~123456";"#;
        let symbols = vec!["0700.HK".to_string(), "600036.SS".to_string()];

        let quotes = parse_tencent_quote_responses(&symbols, response);

        assert_eq!(quotes.len(), 2);
        assert_eq!(quotes[0].as_ref().expect("first quote").price, 461.2);
        assert_eq!(quotes[1].as_ref().expect("second quote").price, 37.55);
    }

    #[test]
    fn tencent_response_matches_batch_payloads_by_quote_code() {
        let response = r#"v_s_sz159201="51~自由现金流ETF华夏~159201~1.090~-0.003~-0.27~2792102~30436~~146.56~ETF~";"#;
        let symbols = vec!["510300".to_string(), "159201".to_string()];

        let quotes = parse_tencent_quote_responses(&symbols, response);

        assert!(quotes[0]
            .as_ref()
            .expect_err("missing 510300 quote")
            .to_string()
            .contains("510300"));
        assert_eq!(quotes[1].as_ref().expect("159201 quote").price, 1.09);
    }

    #[test]
    fn tencent_response_extracts_us_quote_price() {
        let response = r#"v_usPDD="200~拼多多~PDD.OQ~82.26~83.74~83.03~2644638~0~0~0~0~0~0~0~0~0~0~0~0~0~0~0~0~0~0~0~0~0~0~~2026-07-07 12:00:40~-1.48~-1.77~83.87~82.20~USD~2644638";"#;

        let quote = parse_tencent_quote_response("PDD", response).expect("quote");

        assert_eq!(quote.symbol, "PDD");
        assert_eq!(quote.currency.as_deref(), Some("USD"));
        assert_eq!(quote.price, 82.26);
        assert_eq!(quote.volume, Some(2_644_638.0));
        assert_eq!(quote.source, "tencent");
    }
}
