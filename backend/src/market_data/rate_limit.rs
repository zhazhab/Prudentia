use std::{
    future::Future,
    sync::Arc,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::market_data::{ExchangeRate, MarketDataError, MarketDataProvider, MarketQuote};

pub struct RateLimitedMarketDataProvider {
    provider_key: String,
    inner: Arc<dyn MarketDataProvider>,
    min_interval: Duration,
    cooldown: Duration,
    state: Mutex<RateLimitState>,
}

#[derive(Default)]
struct RateLimitState {
    last_request_at: Option<Instant>,
    cooldown_until: Option<Instant>,
}

impl RateLimitedMarketDataProvider {
    pub fn new(
        provider_key: impl Into<String>,
        inner: Arc<dyn MarketDataProvider>,
        limits: ProviderRateLimits,
    ) -> Self {
        Self::with_limits(
            provider_key,
            inner,
            limits.min_interval,
            limits.cooldown_after_throttle,
        )
    }

    pub fn with_limits(
        provider_key: impl Into<String>,
        inner: Arc<dyn MarketDataProvider>,
        min_interval: Duration,
        cooldown: Duration,
    ) -> Self {
        Self {
            provider_key: provider_key.into(),
            inner,
            min_interval,
            cooldown,
            state: Mutex::new(RateLimitState::default()),
        }
    }

    async fn run_limited<T, F, Fut>(&self, operation: F) -> Result<T, MarketDataError>
    where
        F: FnOnce(Arc<dyn MarketDataProvider>) -> Fut,
        Fut: Future<Output = Result<T, MarketDataError>>,
    {
        let mut state = self.state.lock().await;
        self.wait_for_request_slot(&mut state).await?;

        let result = operation(self.inner.clone()).await;
        state.last_request_at = Some(Instant::now());
        if result
            .as_ref()
            .err()
            .is_some_and(MarketDataError::is_rate_limited)
        {
            state.cooldown_until = Some(Instant::now() + self.cooldown);
        }
        result
    }

    async fn wait_for_request_slot(
        &self,
        state: &mut RateLimitState,
    ) -> Result<(), MarketDataError> {
        let now = Instant::now();
        if let Some(cooldown_until) = state.cooldown_until {
            if cooldown_until > now {
                let remaining = cooldown_until.duration_since(now);
                return Err(MarketDataError::RateLimited(format!(
                    "{} market data provider is cooling down for {}s",
                    self.provider_key,
                    remaining.as_secs().max(1)
                )));
            }
            state.cooldown_until = None;
        }

        if let Some(last_request_at) = state.last_request_at {
            let elapsed = now.saturating_duration_since(last_request_at);
            if elapsed < self.min_interval {
                tokio::time::sleep(self.min_interval - elapsed).await;
            }
        }

        Ok(())
    }

    fn cooldown_error(
        &self,
        symbols: &[String],
        error: MarketDataError,
    ) -> Vec<Result<MarketQuote, MarketDataError>> {
        symbols
            .iter()
            .map(|_| Err(MarketDataError::RateLimited(error.to_string())))
            .collect()
    }
}

#[async_trait]
impl MarketDataProvider for RateLimitedMarketDataProvider {
    fn supports_batch_quotes(&self) -> bool {
        self.inner.supports_batch_quotes()
    }

    async fn quote(&self, symbol: &str) -> Result<MarketQuote, MarketDataError> {
        self.run_limited(|inner| {
            let symbol = symbol.to_string();
            async move { inner.quote(&symbol).await }
        })
        .await
    }

    async fn quotes(&self, symbols: &[String]) -> Vec<Result<MarketQuote, MarketDataError>> {
        if symbols.is_empty() {
            return Vec::new();
        }

        if !self.inner.supports_batch_quotes() {
            let mut results = Vec::with_capacity(symbols.len());
            for symbol in symbols {
                results.push(self.quote(symbol).await);
            }
            return results;
        }

        let mut state = self.state.lock().await;
        if let Err(error) = self.wait_for_request_slot(&mut state).await {
            return self.cooldown_error(symbols, error);
        }

        let results = self.inner.quotes(symbols).await;
        state.last_request_at = Some(Instant::now());
        if results
            .iter()
            .filter_map(|result| result.as_ref().err())
            .any(MarketDataError::is_rate_limited)
        {
            state.cooldown_until = Some(Instant::now() + self.cooldown);
        }
        results
    }

    async fn exchange_rate(
        &self,
        from_currency: &str,
        to_currency: &str,
    ) -> Result<ExchangeRate, MarketDataError> {
        self.run_limited(|inner| {
            let from_currency = from_currency.to_string();
            let to_currency = to_currency.to_string();
            async move { inner.exchange_rate(&from_currency, &to_currency).await }
        })
        .await
    }

    async fn exchange_rate_at(
        &self,
        from_currency: &str,
        to_currency: &str,
        rate_date: &str,
    ) -> Result<ExchangeRate, MarketDataError> {
        self.run_limited(|inner| {
            let from_currency = from_currency.to_string();
            let to_currency = to_currency.to_string();
            let rate_date = rate_date.to_string();
            async move {
                inner
                    .exchange_rate_at(&from_currency, &to_currency, &rate_date)
                    .await
            }
        })
        .await
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ProviderRateLimits {
    min_interval: Duration,
    cooldown_after_throttle: Duration,
}

pub fn provider_rate_limits(provider_key: &str) -> ProviderRateLimits {
    match provider_key {
        "alpha_vantage" => ProviderRateLimits {
            min_interval: Duration::from_secs(12),
            cooldown_after_throttle: Duration::from_secs(30 * 60),
        },
        "yahoo" => ProviderRateLimits {
            min_interval: Duration::from_millis(500),
            cooldown_after_throttle: Duration::from_secs(15 * 60),
        },
        "tencent" => ProviderRateLimits {
            min_interval: Duration::from_millis(250),
            cooldown_after_throttle: Duration::from_secs(10 * 60),
        },
        "longbridge" => ProviderRateLimits {
            min_interval: Duration::from_millis(100),
            cooldown_after_throttle: Duration::from_secs(10 * 60),
        },
        _ => ProviderRateLimits {
            min_interval: Duration::from_millis(500),
            cooldown_after_throttle: Duration::from_secs(15 * 60),
        },
    }
}
