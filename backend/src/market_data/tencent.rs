use std::collections::HashMap;

use async_trait::async_trait;
use reqwest::Client;

use crate::{
    market_data::{
        yahoo::yahoo_exchange_rate, ExchangeRate, MarketDataError, MarketDataProvider, MarketQuote,
    },
    time::now_iso,
};

const TENCENT_QUOTE_URL: &str = "https://qt.gtimg.cn/q";
const USER_AGENT: &str = "Mozilla/5.0 Prudentia/0.1";

pub struct TencentMarketDataProvider {
    client: Client,
}

impl TencentMarketDataProvider {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }
}

impl Default for TencentMarketDataProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MarketDataProvider for TencentMarketDataProvider {
    fn supports_batch_quotes(&self) -> bool {
        true
    }

    async fn quote(&self, symbol: &str) -> Result<MarketQuote, MarketDataError> {
        let tencent_symbol = tencent_symbol(symbol).ok_or_else(|| {
            MarketDataError::Provider(format!("unsupported Tencent symbol: {symbol}"))
        })?;
        let body = self
            .client
            .get(TENCENT_QUOTE_URL)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .query(&[("q", tencent_symbol.as_str())])
            .send()
            .await
            .map_err(|err| MarketDataError::Provider(err.to_string()))?
            .error_for_status()
            .map_err(|err| MarketDataError::Provider(err.to_string()))?
            .text()
            .await
            .map_err(|err| MarketDataError::Provider(err.to_string()))?;

        parse_tencent_quote_response(symbol, &body)
    }

    async fn quotes(&self, symbols: &[String]) -> Vec<Result<MarketQuote, MarketDataError>> {
        let mut results = (0..symbols.len()).map(|_| None).collect::<Vec<_>>();
        let mut request_symbols = Vec::new();
        let mut request_indexes = Vec::new();

        for (index, symbol) in symbols.iter().enumerate() {
            match tencent_symbol(symbol) {
                Some(tencent_symbol) => {
                    request_symbols.push(tencent_symbol);
                    request_indexes.push(index);
                }
                None => {
                    results[index] = Some(Err(MarketDataError::Provider(format!(
                        "unsupported Tencent symbol: {symbol}"
                    ))));
                }
            }
        }

        if !request_symbols.is_empty() {
            let request_originals = request_indexes
                .iter()
                .map(|index| symbols[*index].clone())
                .collect::<Vec<_>>();
            let query_symbols = request_symbols.join(",");
            let batch_result = self
                .client
                .get(TENCENT_QUOTE_URL)
                .header(reqwest::header::USER_AGENT, USER_AGENT)
                .query(&[("q", query_symbols.as_str())])
                .send()
                .await
                .map_err(|err| MarketDataError::Provider(err.to_string()))
                .and_then(|response| {
                    response
                        .error_for_status()
                        .map_err(|err| MarketDataError::Provider(err.to_string()))
                });

            match batch_result {
                Ok(response) => match response.text().await {
                    Ok(body) => {
                        for (index, quote_result) in request_indexes
                            .into_iter()
                            .zip(parse_tencent_quote_responses(&request_originals, &body))
                        {
                            results[index] = Some(quote_result);
                        }
                    }
                    Err(error) => {
                        let message = error.to_string();
                        for index in request_indexes {
                            results[index] = Some(Err(MarketDataError::Provider(message.clone())));
                        }
                    }
                },
                Err(error) => {
                    let message = error.to_string();
                    for index in request_indexes {
                        results[index] = Some(Err(MarketDataError::Provider(message.clone())));
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
                        "{}: missing Tencent batch quote result",
                        symbols[index]
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
        yahoo_exchange_rate(&self.client, from_currency, to_currency, "tencent:yahoo_fx").await
    }
}

pub fn tencent_symbol(symbol: &str) -> Option<String> {
    let normalized = symbol.trim().to_ascii_uppercase();
    if let Some(code) = normalized.strip_suffix(".HK") {
        return numeric_code(code, 5).map(|code| format!("s_hk{code}"));
    }
    if let Some(code) = normalized.strip_suffix(".SS") {
        if code == "000001" {
            return Some("sh000001".to_string());
        }
        return numeric_code(code, 6).map(|code| format!("s_sh{code}"));
    }
    if let Some(code) = normalized.strip_suffix(".SZ") {
        if code.starts_with("399") {
            return numeric_code(code, 6).map(|code| format!("sz{code}"));
        }
        return numeric_code(code, 6).map(|code| format!("s_sz{code}"));
    }
    if normalized.chars().all(|char| char.is_ascii_digit()) {
        let prefix = if normalized.starts_with('5') || normalized.starts_with('6') {
            "s_sh"
        } else {
            "s_sz"
        };
        return numeric_code(&normalized, 6).map(|code| format!("{prefix}{code}"));
    }
    Some(format!("us{normalized}"))
}

pub fn parse_tencent_quote_response(
    original_symbol: &str,
    body: &str,
) -> Result<MarketQuote, MarketDataError> {
    let quoted = body
        .split('"')
        .nth(1)
        .ok_or_else(|| MarketDataError::Provider("missing Tencent quote payload".to_string()))?;
    parse_tencent_quote_payload(original_symbol, quoted)
}

pub fn parse_tencent_quote_responses(
    original_symbols: &[String],
    body: &str,
) -> Vec<Result<MarketQuote, MarketDataError>> {
    let payloads = tencent_payloads_by_code(body);
    original_symbols
        .iter()
        .map(|symbol| {
            let tencent_symbol = tencent_symbol(symbol).ok_or_else(|| {
                MarketDataError::Provider(format!("unsupported Tencent symbol: {symbol}"))
            })?;
            let response_key = format!("v_{tencent_symbol}");
            payloads
                .get(response_key.as_str())
                .map(|payload| parse_tencent_quote_payload(symbol, payload))
                .unwrap_or_else(|| {
                    Err(MarketDataError::Provider(format!(
                        "{symbol}: missing Tencent quote payload"
                    )))
                })
        })
        .collect()
}

fn tencent_payloads_by_code(body: &str) -> HashMap<&str, &str> {
    body.split(';')
        .filter_map(|statement| {
            let statement = statement.trim();
            let (name, quoted) = statement.split_once("=\"")?;
            let payload = quoted.strip_suffix('"')?;
            Some((name.trim(), payload))
        })
        .collect()
}

fn parse_tencent_quote_payload(
    original_symbol: &str,
    quoted: &str,
) -> Result<MarketQuote, MarketDataError> {
    let fields: Vec<&str> = quoted.split('~').collect();
    let price = parse_field(&fields, 3, "price")?;
    if price <= 0.0 {
        return Err(MarketDataError::Provider(format!(
            "{original_symbol}: Tencent quote price is empty"
        )));
    }

    Ok(MarketQuote {
        symbol: original_symbol.trim().to_ascii_uppercase(),
        price,
        currency: Some(tencent_currency(original_symbol, &fields)),
        volume: fields.get(6).and_then(|value| value.parse::<f64>().ok()),
        source: "tencent".to_string(),
        updated_at: now_iso(),
    })
}

fn numeric_code(code: &str, width: usize) -> Option<String> {
    let trimmed = code.trim_start_matches('0');
    if trimmed.is_empty() || !trimmed.chars().all(|char| char.is_ascii_digit()) {
        return None;
    }
    Some(format!("{:0>width$}", trimmed))
}

fn parse_field(fields: &[&str], index: usize, label: &str) -> Result<f64, MarketDataError> {
    fields
        .get(index)
        .ok_or_else(|| MarketDataError::Provider(format!("missing Tencent {label} field")))?
        .parse::<f64>()
        .map_err(|_| MarketDataError::Provider(format!("invalid Tencent {label} field")))
}

fn tencent_currency(original_symbol: &str, fields: &[&str]) -> String {
    if is_tencent_index_symbol(original_symbol) {
        return "CNY".to_string();
    }

    fields
        .get(37)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_uppercase)
        .unwrap_or_else(|| {
            let normalized = original_symbol.trim().to_ascii_uppercase();
            if normalized.ends_with(".HK") {
                "HKD".to_string()
            } else if normalized.ends_with(".SS")
                || normalized.ends_with(".SZ")
                || normalized.chars().all(|char| char.is_ascii_digit())
            {
                "CNY".to_string()
            } else {
                "USD".to_string()
            }
        })
}

fn is_tencent_index_symbol(original_symbol: &str) -> bool {
    let normalized = original_symbol.trim().to_ascii_uppercase();
    normalized == "000001.SS"
        || normalized
            .strip_suffix(".SZ")
            .is_some_and(|code| code.starts_with("399"))
}
