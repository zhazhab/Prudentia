use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use longbridge::{portfolio::PortfolioContext, quote::QuoteContext, Config};

use crate::market_data::{ExchangeRate, MarketDataError, MarketDataProvider, MarketQuote};

pub struct LongbridgeMarketDataProvider {
    quote: QuoteContext,
    portfolio: PortfolioContext,
}

impl LongbridgeMarketDataProvider {
    pub fn new() -> Result<Self, MarketDataError> {
        let config =
            Config::from_apikey_env().map_err(|err| MarketDataError::Provider(err.to_string()))?;
        let config = Arc::new(config.dont_print_quote_packages());
        let (quote, _) = QuoteContext::new(config.clone());
        let portfolio = PortfolioContext::new(config);
        Ok(Self { quote, portfolio })
    }
}

#[async_trait]
impl MarketDataProvider for LongbridgeMarketDataProvider {
    fn supports_batch_quotes(&self) -> bool {
        true
    }

    async fn quote(&self, symbol: &str) -> Result<MarketQuote, MarketDataError> {
        let symbol = symbol.to_string();
        self.quotes(&[symbol])
            .await
            .into_iter()
            .next()
            .unwrap_or_else(|| {
                Err(MarketDataError::Provider(
                    "missing Longbridge quote result".to_string(),
                ))
            })
    }

    async fn quotes(&self, symbols: &[String]) -> Vec<Result<MarketQuote, MarketDataError>> {
        if symbols.is_empty() {
            return Vec::new();
        }
        let longbridge_symbols = symbols
            .iter()
            .map(|symbol| longbridge_symbol(symbol))
            .collect::<Vec<_>>();
        let quotes = self
            .quote
            .quote(longbridge_symbols.clone())
            .await
            .map_err(|err| MarketDataError::Provider(err.to_string()));

        match quotes {
            Ok(quotes) => {
                let mut quote_by_symbol = quotes
                    .into_iter()
                    .map(|quote| (quote.symbol.to_ascii_uppercase(), quote))
                    .collect::<HashMap<_, _>>();
                symbols
                    .iter()
                    .zip(longbridge_symbols)
                    .map(|(original_symbol, longbridge_symbol)| {
                        quote_by_symbol
                            .remove(&longbridge_symbol.to_ascii_uppercase())
                            .ok_or_else(|| {
                                MarketDataError::Provider(format!(
                                    "{original_symbol}: missing Longbridge quote"
                                ))
                            })
                            .and_then(|quote| {
                                market_quote_from_longbridge_quote(
                                    original_symbol,
                                    &longbridge_symbol,
                                    quote,
                                )
                            })
                    })
                    .collect()
            }
            Err(error) => symbols
                .iter()
                .map(|_| Err(MarketDataError::Provider(error.to_string())))
                .collect(),
        }
    }

    async fn exchange_rate(
        &self,
        from_currency: &str,
        to_currency: &str,
    ) -> Result<ExchangeRate, MarketDataError> {
        let from = normalize_currency(from_currency);
        let to = normalize_currency(to_currency);
        if from == to {
            return Ok(ExchangeRate {
                from_currency: from,
                to_currency: to,
                rate: 1.0,
                source: "longbridge".to_string(),
                updated_at: crate::time::now_iso(),
            });
        }
        let rates = self
            .portfolio
            .exchange_rate()
            .await
            .map_err(|err| MarketDataError::Provider(err.to_string()))?;
        let rate = rates
            .exchanges
            .iter()
            .find(|rate| {
                normalize_currency(&rate.base_currency) == from
                    && normalize_currency(&rate.other_currency) == to
            })
            .or_else(|| {
                rates.exchanges.iter().find(|rate| {
                    normalize_currency(&rate.base_currency) == to
                        && normalize_currency(&rate.other_currency) == from
                })
            })
            .ok_or_else(|| {
                MarketDataError::Provider(format!("missing Longbridge FX rate for {from}/{to}"))
            })?;
        let mut value = rate.average_rate;
        if normalize_currency(&rate.base_currency) == to
            && normalize_currency(&rate.other_currency) == from
        {
            value = 1.0 / value;
        }

        Ok(ExchangeRate {
            from_currency: from,
            to_currency: to,
            rate: value,
            source: "longbridge".to_string(),
            updated_at: crate::time::now_iso(),
        })
    }
}

fn market_quote_from_longbridge_quote(
    original_symbol: &str,
    longbridge_symbol: &str,
    quote: longbridge::quote::SecurityQuote,
) -> Result<MarketQuote, MarketDataError> {
    let price = quote.last_done.to_string().parse::<f64>().map_err(|_| {
        MarketDataError::Provider(format!("{original_symbol}: invalid Longbridge price"))
    })?;

    Ok(MarketQuote {
        symbol: original_symbol.trim().to_ascii_uppercase(),
        price,
        currency: Some(currency_for_longbridge_symbol(longbridge_symbol)),
        volume: Some(quote.volume as f64),
        source: "longbridge".to_string(),
        updated_at: quote.timestamp.to_string(),
    })
}

pub fn longbridge_symbol(symbol: &str) -> String {
    let normalized = symbol.trim().to_ascii_uppercase();
    if let Some(code) = normalized.strip_suffix(".HK") {
        return format!("{}.HK", trim_leading_zeroes(code));
    }
    if let Some(code) = normalized.strip_suffix(".SS") {
        return format!("{code}.SH");
    }
    if normalized.ends_with(".SZ") {
        return normalized;
    }
    if normalized.chars().all(|char| char.is_ascii_digit()) {
        return if normalized.starts_with('6') {
            format!("{normalized}.SH")
        } else {
            format!("{normalized}.SZ")
        };
    }
    if normalized.contains('.') {
        normalized
    } else {
        format!("{normalized}.US")
    }
}

fn trim_leading_zeroes(code: &str) -> &str {
    let trimmed = code.trim_start_matches('0');
    if trimmed.is_empty() {
        code
    } else {
        trimmed
    }
}

fn currency_for_longbridge_symbol(symbol: &str) -> String {
    if symbol.ends_with(".HK") {
        "HKD".to_string()
    } else if symbol.ends_with(".SH") || symbol.ends_with(".SZ") {
        "CNY".to_string()
    } else {
        "USD".to_string()
    }
}

fn normalize_currency(currency: impl AsRef<str>) -> String {
    currency.as_ref().trim().to_ascii_uppercase()
}
