use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

use crate::{
    market_data::{ExchangeRate, MarketDataError, MarketDataProvider, MarketQuote},
    time::now_iso,
};

const YAHOO_CHART_URL: &str = "https://query1.finance.yahoo.com/v8/finance/chart";
const USER_AGENT: &str = "Mozilla/5.0 Prudentia/0.1";

pub struct YahooMarketDataProvider {
    client: Client,
}

impl YahooMarketDataProvider {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }
}

impl Default for YahooMarketDataProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MarketDataProvider for YahooMarketDataProvider {
    async fn quote(&self, symbol: &str) -> Result<MarketQuote, MarketDataError> {
        let body = yahoo_chart_body(&self.client, symbol).await?;
        parse_yahoo_chart_response(symbol, &body)
    }

    async fn exchange_rate(
        &self,
        from_currency: &str,
        to_currency: &str,
    ) -> Result<ExchangeRate, MarketDataError> {
        yahoo_exchange_rate(&self.client, from_currency, to_currency, "yahoo").await
    }
}

pub async fn yahoo_exchange_rate(
    client: &Client,
    from_currency: &str,
    to_currency: &str,
    source: &str,
) -> Result<ExchangeRate, MarketDataError> {
    let from = normalize_currency(from_currency);
    let to = normalize_currency(to_currency);
    if from == to {
        return Ok(ExchangeRate {
            from_currency: from,
            to_currency: to,
            rate: 1.0,
            source: source.to_string(),
            updated_at: now_iso(),
        });
    }

    let yahoo_symbol = format!("{from}{to}=X");
    let body = yahoo_chart_body(client, &yahoo_symbol).await?;
    let quote = parse_yahoo_chart_response(&yahoo_symbol, &body)?;

    Ok(ExchangeRate {
        from_currency: from,
        to_currency: to,
        rate: quote.price,
        source: source.to_string(),
        updated_at: quote.updated_at,
    })
}

pub fn parse_yahoo_chart_response(
    fallback_symbol: &str,
    body: &str,
) -> Result<MarketQuote, MarketDataError> {
    let response: YahooChartResponse =
        serde_json::from_str(body).map_err(|err| MarketDataError::Provider(err.to_string()))?;
    let result = response
        .chart
        .result
        .and_then(|mut results| results.drain(..).next())
        .ok_or_else(|| {
            MarketDataError::Provider(
                response
                    .chart
                    .error
                    .map(|error| error.description)
                    .unwrap_or_else(|| "missing Yahoo chart result".to_string()),
            )
        })?;
    let price = result.meta.regular_market_price.ok_or_else(|| {
        MarketDataError::Provider(format!(
            "{fallback_symbol}: missing Yahoo regularMarketPrice"
        ))
    })?;
    let volume = result
        .indicators
        .and_then(|indicators| indicators.quote.into_iter().next())
        .and_then(|quote| quote.volume.into_iter().flatten().next())
        .map(|value| value as f64);

    Ok(MarketQuote {
        symbol: result
            .meta
            .symbol
            .unwrap_or_else(|| fallback_symbol.to_ascii_uppercase()),
        price,
        currency: result.meta.currency.map(normalize_currency),
        volume,
        source: "yahoo".to_string(),
        updated_at: result
            .meta
            .regular_market_time
            .map(unix_timestamp_to_iso)
            .unwrap_or_else(now_iso),
    })
}

async fn yahoo_chart_body(client: &Client, symbol: &str) -> Result<String, MarketDataError> {
    client
        .get(format!("{YAHOO_CHART_URL}/{symbol}"))
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .query(&[("range", "1d"), ("interval", "1d")])
        .send()
        .await
        .map_err(|err| MarketDataError::Provider(err.to_string()))?
        .error_for_status()
        .map_err(|err| MarketDataError::Provider(err.to_string()))?
        .text()
        .await
        .map_err(|err| MarketDataError::Provider(err.to_string()))
}

fn normalize_currency(currency: impl AsRef<str>) -> String {
    currency.as_ref().trim().to_ascii_uppercase()
}

fn unix_timestamp_to_iso(timestamp: i64) -> String {
    chrono::DateTime::from_timestamp(timestamp, 0)
        .map(|value| value.to_rfc3339())
        .unwrap_or_else(now_iso)
}

#[derive(Deserialize)]
struct YahooChartResponse {
    chart: YahooChart,
}

#[derive(Deserialize)]
struct YahooChart {
    result: Option<Vec<YahooChartResult>>,
    error: Option<YahooChartError>,
}

#[derive(Deserialize)]
struct YahooChartError {
    description: String,
}

#[derive(Deserialize)]
struct YahooChartResult {
    meta: YahooChartMeta,
    indicators: Option<YahooIndicators>,
}

#[derive(Deserialize)]
struct YahooChartMeta {
    currency: Option<String>,
    symbol: Option<String>,
    #[serde(rename = "regularMarketPrice")]
    regular_market_price: Option<f64>,
    #[serde(rename = "regularMarketTime")]
    regular_market_time: Option<i64>,
}

#[derive(Deserialize)]
struct YahooIndicators {
    quote: Vec<YahooQuoteSeries>,
}

#[derive(Deserialize)]
struct YahooQuoteSeries {
    #[serde(default)]
    volume: Vec<Option<i64>>,
}
