use chrono::{Datelike, TimeZone};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PerformancePeriod {
    Month,
    Year,
    SinceInception,
}

#[derive(Debug, Clone, Copy)]
struct BenchmarkDefinition {
    key: &'static str,
    label: &'static str,
    symbol: &'static str,
    currency: &'static str,
}

#[derive(Debug, Clone)]
struct PerformanceSnapshotRow {
    captured_at: String,
    total_market_value_base: f64,
}

#[derive(Debug, Clone)]
struct PerformanceCashFlowRow {
    occurred_at: String,
    amount_base: f64,
}

#[derive(Debug, Clone)]
struct BenchmarkSnapshotRow {
    captured_at: String,
    value_base: Option<f64>,
    stale: bool,
    error: Option<String>,
}

const PRICE_REFRESH_STATE_KEY: &str = "portfolio_prices";
const PRICE_REFRESH_SNAPSHOT_SOURCE: &str = "price_refresh";

pub async fn record_portfolio_performance_snapshot(
    pool: &SqlitePool,
    market_data: Arc<dyn MarketDataProvider>,
    source: &str,
) -> AppResult<()> {
    let summary = summary(pool).await?;
    let snapshot_id = Uuid::new_v4().to_string();
    let captured_at = now_iso();

    sqlx::query(
        r#"
        INSERT INTO portfolio_performance_snapshots (
            id, captured_at, source, base_currency, total_market_value_base,
            total_cost_base, total_unrealized_pnl_base
        )
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&snapshot_id)
    .bind(&captured_at)
    .bind(source)
    .bind(&summary.base_currency)
    .bind(summary.total_market_value_base)
    .bind(summary.total_cost_base)
    .bind(summary.total_unrealized_pnl_base)
    .execute(pool)
    .await?;

    record_position_snapshots(pool, &snapshot_id, &captured_at, source).await?;
    record_automatic_trade_cash_flow(
        pool,
        &captured_at,
        source,
        summary.total_market_value_base,
    )
    .await?;

    if should_record_benchmark_snapshots(source) {
        let benchmarks = benchmark_definitions();
        let benchmark_symbols = benchmarks
            .iter()
            .map(|benchmark| benchmark.symbol.to_string())
            .collect::<Vec<_>>();
        let mut quote_results = market_data.quotes(&benchmark_symbols).await.into_iter();

        for benchmark in benchmarks {
            let quote_result = quote_results.next().unwrap_or_else(|| {
                Err(crate::market_data::MarketDataError::Provider(format!(
                    "{}: missing batch quote result",
                    benchmark.symbol
                )))
            });
            record_benchmark_snapshot(
                pool,
                market_data.clone(),
                &snapshot_id,
                &captured_at,
                benchmark,
                quote_result,
            )
            .await?;
        }
    }

    tracing::info!(
        snapshot_id,
        source,
        total_market_value_base = summary.total_market_value_base,
        "portfolio performance snapshot recorded"
    );

    Ok(())
}

fn should_record_benchmark_snapshots(source: &str) -> bool {
    source == PRICE_REFRESH_SNAPSHOT_SOURCE
}

pub async fn portfolio_performance(
    pool: &SqlitePool,
    query: PortfolioPerformanceQuery,
) -> AppResult<PortfolioPerformanceResponse> {
    let period = parse_performance_period(query.period.as_deref())?;
    let period_start = period_start_utc(period);
    let snapshots = load_performance_snapshots(pool, period_start.as_deref()).await?;
    let cash_flows = load_performance_cash_flows(pool, period_start.as_deref()).await?;
    let portfolio = portfolio_metric(&snapshots, &cash_flows);
    let series = portfolio_series(&snapshots, &cash_flows);
    let benchmarks = load_benchmark_performance(pool, period, period_start.as_deref()).await?;
    let first_snapshot = snapshots.first();
    let last_snapshot = snapshots.last();

    Ok(PortfolioPerformanceResponse {
        period: period.as_str().to_string(),
        base_currency: BASE_CURRENCY.to_string(),
        start_date: first_snapshot.map(|snapshot| snapshot.captured_at.clone()),
        end_date: last_snapshot.map(|snapshot| snapshot.captured_at.clone()),
        partial_period: is_partial_period(period, period_start.as_deref(), first_snapshot),
        portfolio,
        series,
        benchmarks,
        updated_at: now_iso(),
    })
}

pub async fn refresh_prices_if_due(
    pool: &SqlitePool,
    market_data: Arc<dyn MarketDataProvider>,
    ttl: Duration,
) -> AppResult<Option<PriceRefreshResult>> {
    if !portfolio_price_refresh_due(pool, ttl).await? {
        tracing::debug!("portfolio price refresh skipped; daily TTL is still fresh");
        return Ok(None);
    }

    record_price_refresh_attempt(pool, "running", None, false).await?;
    match refresh_prices(pool, market_data).await {
        Ok(result) => {
            record_price_refresh_attempt(pool, "success", None, true).await?;
            Ok(Some(result))
        }
        Err(error) => {
            record_price_refresh_attempt(pool, "failed", Some(error.to_string()), false).await?;
            Err(error)
        }
    }
}

async fn record_benchmark_snapshot(
    pool: &SqlitePool,
    market_data: Arc<dyn MarketDataProvider>,
    snapshot_id: &str,
    captured_at: &str,
    benchmark: BenchmarkDefinition,
    quote_result: Result<crate::market_data::MarketQuote, crate::market_data::MarketDataError>,
) -> AppResult<()> {
    let currency = benchmark.currency.to_string();
    let mut price = None;
    let mut source = None;
    let mut fx_rate = None;
    let mut value_base = None;
    let stale;
    let error;

    match quote_result {
        Ok(quote) => {
            if is_mock_market_data_source(&quote.source) {
                tracing::warn!(
                    benchmark_key = benchmark.key,
                    symbol = benchmark.symbol,
                    "portfolio benchmark mock quote ignored"
                );
                stale = true;
                error = Some(format!(
                    "{}: mock quote provider does not update benchmark prices",
                    benchmark.symbol
                ));
            } else {
                price = Some(quote.price);
                source = Some(quote.source);

                match benchmark_fx_rate(pool, market_data, &currency).await? {
                    Some((rate, is_stale, fx_error)) => {
                        fx_rate = Some(rate);
                        value_base = Some(quote.price * rate);
                        stale = is_stale;
                        error = fx_error;
                    }
                    None => {
                        stale = true;
                        error = Some(format!("missing FX rate for {currency}/{BASE_CURRENCY}"));
                    }
                }
            };
        }
        Err(quote_error) => {
            tracing::warn!(
                benchmark_key = benchmark.key,
                symbol = benchmark.symbol,
                error = ?quote_error,
                "portfolio benchmark quote unavailable"
            );
            stale = true;
            error = Some(quote_error.to_string());
        }
    }

    sqlx::query(
        r#"
        INSERT INTO portfolio_benchmark_snapshots (
            id, snapshot_id, benchmark_key, label, symbol, currency, price,
            fx_rate, value_base, source, stale, error, captured_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(snapshot_id)
    .bind(benchmark.key)
    .bind(benchmark.label)
    .bind(benchmark.symbol)
    .bind(currency)
    .bind(price)
    .bind(fx_rate)
    .bind(value_base)
    .bind(source)
    .bind(stale)
    .bind(error)
    .bind(captured_at)
    .execute(pool)
    .await?;

    Ok(())
}

async fn benchmark_fx_rate(
    pool: &SqlitePool,
    market_data: Arc<dyn MarketDataProvider>,
    currency: &str,
) -> AppResult<Option<(f64, bool, Option<String>)>> {
    if currency.eq_ignore_ascii_case(BASE_CURRENCY) {
        upsert_fx_rate(
            pool,
            &PortfolioFxRate {
                from_currency: BASE_CURRENCY.to_string(),
                to_currency: BASE_CURRENCY.to_string(),
                rate: 1.0,
                source: "identity".to_string(),
                updated_at: now_iso(),
                stale: false,
            },
        )
        .await?;
        return Ok(Some((1.0, false, None)));
    }

    match market_data.exchange_rate(currency, BASE_CURRENCY).await {
        Ok(rate) => {
            upsert_fx_rate(
                pool,
                &PortfolioFxRate {
                    from_currency: rate.from_currency.to_ascii_uppercase(),
                    to_currency: rate.to_currency.to_ascii_uppercase(),
                    rate: rate.rate,
                    source: rate.source,
                    updated_at: rate.updated_at,
                    stale: false,
                },
            )
            .await?;
            Ok(Some((rate.rate, false, None)))
        }
        Err(fx_error) => {
            let fx_rates = load_fx_rates(pool).await?;
            let rate = fx_rates
                .iter()
                .find(|rate| {
                    rate.from_currency.eq_ignore_ascii_case(currency)
                        && rate.to_currency.eq_ignore_ascii_case(BASE_CURRENCY)
                })
                .map(|rate| rate.rate);
            if rate.is_some() {
                tracing::warn!(
                    from_currency = currency,
                    to_currency = BASE_CURRENCY,
                    error = ?fx_error,
                    "using stale benchmark FX rate"
                );
            } else {
                tracing::warn!(
                    from_currency = currency,
                    to_currency = BASE_CURRENCY,
                    error = ?fx_error,
                    "portfolio benchmark FX unavailable"
                );
            }
            Ok(rate.map(|value| (value, true, Some(fx_error.to_string()))))
        }
    }
}

async fn load_performance_snapshots(
    pool: &SqlitePool,
    period_start: Option<&str>,
) -> AppResult<Vec<PerformanceSnapshotRow>> {
    let rows = if let Some(period_start) = period_start {
        sqlx::query(
            r#"
            SELECT captured_at, total_market_value_base
            FROM portfolio_performance_snapshots
            WHERE captured_at >= ?
            ORDER BY captured_at ASC, id ASC
            "#,
        )
        .bind(period_start)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            r#"
            SELECT captured_at, total_market_value_base
            FROM portfolio_performance_snapshots
            ORDER BY captured_at ASC, id ASC
            "#,
        )
        .fetch_all(pool)
        .await?
    };

    rows.into_iter()
        .map(|row| {
            Ok(PerformanceSnapshotRow {
                captured_at: row.try_get("captured_at")?,
                total_market_value_base: row.try_get("total_market_value_base")?,
            })
        })
        .collect()
}

async fn load_performance_cash_flows(
    pool: &SqlitePool,
    period_start: Option<&str>,
) -> AppResult<Vec<PerformanceCashFlowRow>> {
    let rows = if let Some(period_start) = period_start {
        sqlx::query(
            r#"
            SELECT occurred_at, amount_base
            FROM portfolio_cash_flows
            WHERE occurred_at >= ?
            ORDER BY occurred_at ASC, id ASC
            "#,
        )
        .bind(period_start)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            r#"
            SELECT occurred_at, amount_base
            FROM portfolio_cash_flows
            ORDER BY occurred_at ASC, id ASC
            "#,
        )
        .fetch_all(pool)
        .await?
    };

    rows.into_iter()
        .map(|row| {
            Ok(PerformanceCashFlowRow {
                occurred_at: row.try_get("occurred_at")?,
                amount_base: row.try_get("amount_base")?,
            })
        })
        .collect()
}

async fn load_benchmark_performance(
    pool: &SqlitePool,
    period: PerformancePeriod,
    period_start: Option<&str>,
) -> AppResult<Vec<BenchmarkPerformance>> {
    let mut benchmarks = Vec::new();
    for benchmark in benchmark_definitions() {
        let rows =
            load_benchmark_snapshots(pool, benchmark.key, benchmark.symbol, period_start).await?;
        let start = rows
            .iter()
            .find_map(|row| row.value_base.map(|value| (row.captured_at.clone(), value)));
        let end = rows
            .iter()
            .rev()
            .find_map(|row| row.value_base.map(|value| (row.captured_at.clone(), value)));
        let start_value = start.as_ref().map(|(_, value)| *value);
        let end_value = end.as_ref().map(|(_, value)| *value);
        let return_pct = start_value
            .zip(end_value)
            .and_then(|(start, end)| percentage_return(start, end));
        let annualized_return_pct = start
            .as_ref()
            .zip(end.as_ref())
            .and_then(|((start_at, start_value), (end_at, end_value))| {
                annualized_return(*start_value, *end_value, start_at, end_at)
            });
        let latest_stale = rows.last().map(|row| row.stale).unwrap_or(true);
        let latest_error = rows.last().and_then(|row| row.error.clone());
        let series_start = start.clone();
        let series = rows
            .into_iter()
            .map(|row| BenchmarkPerformancePoint {
                captured_at: row.captured_at.clone(),
                value_base: row.value_base,
                return_pct: series_start
                    .as_ref()
                    .zip(row.value_base)
                    .and_then(|((_, start), value)| percentage_return(*start, value)),
                annualized_return_pct: series_start
                    .as_ref()
                    .zip(row.value_base)
                    .and_then(|((start_at, start), value)| {
                        annualized_return(*start, value, start_at, &row.captured_at)
                    }),
                stale: row.stale,
                error: row.error,
            })
            .collect::<Vec<_>>();
        let available = start_value.is_some() && end_value.is_some();

        benchmarks.push(BenchmarkPerformance {
            key: benchmark.key.to_string(),
            label: benchmark.label.to_string(),
            symbol: benchmark.symbol.to_string(),
            available,
            stale: latest_stale,
            start_value_base: start_value,
            end_value_base: end_value,
            return_pct,
            annualized_return_pct,
            error: latest_error
                .filter(|_| !available || period != PerformancePeriod::SinceInception),
            series,
        });
    }

    Ok(benchmarks)
}

async fn load_benchmark_snapshots(
    pool: &SqlitePool,
    benchmark_key: &str,
    symbol: &str,
    period_start: Option<&str>,
) -> AppResult<Vec<BenchmarkSnapshotRow>> {
    let rows = if let Some(period_start) = period_start {
        sqlx::query(
            r#"
            SELECT captured_at, value_base, source, stale, error
            FROM portfolio_benchmark_snapshots
            WHERE benchmark_key = ? AND symbol = ? AND captured_at >= ?
            ORDER BY captured_at ASC, id ASC
            "#,
        )
        .bind(benchmark_key)
        .bind(symbol)
        .bind(period_start)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            r#"
            SELECT captured_at, value_base, source, stale, error
            FROM portfolio_benchmark_snapshots
            WHERE benchmark_key = ? AND symbol = ?
            ORDER BY captured_at ASC, id ASC
            "#,
        )
        .bind(benchmark_key)
        .bind(symbol)
        .fetch_all(pool)
        .await?
    };

    rows.into_iter()
        .map(|row| {
            let source = row.try_get::<Option<String>, _>("source")?;
            let is_mock = source
                .as_deref()
                .is_some_and(is_mock_market_data_source);
            let error = row.try_get::<Option<String>, _>("error")?;
            Ok(BenchmarkSnapshotRow {
                captured_at: row.try_get("captured_at")?,
                value_base: if is_mock {
                    None
                } else {
                    row.try_get("value_base")?
                },
                stale: row.try_get::<i64, _>("stale")? != 0 || is_mock,
                error: if is_mock {
                    Some(error.unwrap_or_else(|| {
                        "mock quote provider does not update benchmark prices".to_string()
                    }))
                } else {
                    error
                },
            })
        })
        .collect()
}

async fn portfolio_price_refresh_due(pool: &SqlitePool, ttl: Duration) -> AppResult<bool> {
    let attempted_at = sqlx::query(
        "SELECT attempted_at FROM portfolio_refresh_state WHERE key = ?",
    )
    .bind(PRICE_REFRESH_STATE_KEY)
    .fetch_optional(pool)
    .await?
    .and_then(|row| row.try_get::<Option<String>, _>("attempted_at").ok())
    .flatten();

    let Some(attempted_at) = attempted_at else {
        return Ok(true);
    };

    let Ok(attempted_at) = chrono::DateTime::parse_from_rfc3339(&attempted_at) else {
        return Ok(true);
    };
    let elapsed = chrono::Utc::now()
        .signed_duration_since(attempted_at.with_timezone(&chrono::Utc))
        .to_std()
        .unwrap_or(Duration::ZERO);

    Ok(elapsed >= ttl)
}

async fn record_price_refresh_attempt(
    pool: &SqlitePool,
    status: &str,
    error: Option<String>,
    succeeded: bool,
) -> AppResult<()> {
    let now = now_iso();
    let succeeded_at = if succeeded { Some(now.clone()) } else { None };
    sqlx::query(
        r#"
        INSERT INTO portfolio_refresh_state (key, attempted_at, succeeded_at, status, error)
        VALUES (?, ?, ?, ?, ?)
        ON CONFLICT(key) DO UPDATE SET
            attempted_at = excluded.attempted_at,
            succeeded_at = COALESCE(excluded.succeeded_at, portfolio_refresh_state.succeeded_at),
            status = excluded.status,
            error = excluded.error
        "#,
    )
    .bind(PRICE_REFRESH_STATE_KEY)
    .bind(now)
    .bind(succeeded_at)
    .bind(status)
    .bind(error)
    .execute(pool)
    .await?;

    Ok(())
}

fn portfolio_metric(
    snapshots: &[PerformanceSnapshotRow],
    cash_flows: &[PerformanceCashFlowRow],
) -> PortfolioPerformanceMetric {
    let start_snapshot = snapshots.first();
    let end_snapshot = snapshots.last();
    let start = start_snapshot.map(|snapshot| snapshot.total_market_value_base);
    let end = end_snapshot.map(|snapshot| snapshot.total_market_value_base);
    let net_cash_flow = start_snapshot
        .zip(end_snapshot)
        .map(|(start, end)| cash_flow_sum_between(cash_flows, &start.captured_at, &end.captured_at))
        .unwrap_or(0.0);
    let profit_loss = start.zip(end).map(|(start, end)| end - start - net_cash_flow);
    let simple_return_pct = start
        .zip(end)
        .and_then(|(start, end)| percentage_return(start, end));
    let return_pct = time_weighted_return(snapshots, cash_flows);
    let annualized_return_pct = start_snapshot
        .zip(end_snapshot)
        .zip(return_pct)
        .and_then(|((start_snapshot, end_snapshot), return_pct)| {
            annualized_return_from_period_return(
                return_pct,
                &start_snapshot.captured_at,
                &end_snapshot.captured_at,
            )
        });

    PortfolioPerformanceMetric {
        start_value_base: start,
        end_value_base: end,
        profit_loss_base: profit_loss,
        net_cash_flow_base: net_cash_flow,
        return_pct,
        simple_return_pct,
        annualized_return_pct,
        return_method: "time_weighted".to_string(),
    }
}

fn portfolio_series(
    snapshots: &[PerformanceSnapshotRow],
    cash_flows: &[PerformanceCashFlowRow],
) -> Vec<PortfolioPerformancePoint> {
    let Some(start_snapshot) = snapshots.first() else {
        return Vec::new();
    };

    let mut previous_snapshot = start_snapshot;
    let mut cumulative_factor = Some(1.0);
    let mut cumulative_cash_flow = 0.0;

    snapshots
        .iter()
        .enumerate()
        .map(|(index, snapshot)| {
            if index > 0 {
                let interval_flow =
                    cash_flow_sum_between(cash_flows, &previous_snapshot.captured_at, &snapshot.captured_at);
                cumulative_cash_flow += interval_flow;
                cumulative_factor = cumulative_factor.and_then(|factor| {
                    period_return_factor(
                        previous_snapshot.total_market_value_base,
                        snapshot.total_market_value_base,
                        interval_flow,
                    )
                    .map(|period_factor| factor * period_factor)
                });
                previous_snapshot = snapshot;
            }

            let return_pct = cumulative_factor.map(|factor| factor - 1.0);
            PortfolioPerformancePoint {
                captured_at: snapshot.captured_at.clone(),
                value_base: snapshot.total_market_value_base,
                profit_loss_base: Some(
                    snapshot.total_market_value_base
                        - start_snapshot.total_market_value_base
                        - cumulative_cash_flow,
                ),
                net_cash_flow_base: cumulative_cash_flow,
                return_pct,
                simple_return_pct: percentage_return(
                    start_snapshot.total_market_value_base,
                    snapshot.total_market_value_base,
                ),
                annualized_return_pct: return_pct.and_then(|value| {
                    annualized_return_from_period_return(
                        value,
                        &start_snapshot.captured_at,
                        &snapshot.captured_at,
                    )
                }),
            }
        })
        .collect()
}

fn time_weighted_return(
    snapshots: &[PerformanceSnapshotRow],
    cash_flows: &[PerformanceCashFlowRow],
) -> Option<f64> {
    portfolio_series(snapshots, cash_flows)
        .last()
        .and_then(|point| point.return_pct)
}

fn percentage_return(start: f64, end: f64) -> Option<f64> {
    if start.abs() < f64::EPSILON {
        return None;
    }
    Some(end / start - 1.0)
}

fn annualized_return(start: f64, end: f64, start_at: &str, end_at: &str) -> Option<f64> {
    let return_pct = percentage_return(start, end)?;
    annualized_return_from_period_return(return_pct, start_at, end_at)
}

fn is_partial_period(
    period: PerformancePeriod,
    period_start: Option<&str>,
    first_snapshot: Option<&PerformanceSnapshotRow>,
) -> bool {
    if period == PerformancePeriod::SinceInception {
        return false;
    }

    match (period_start, first_snapshot) {
        (Some(period_start), Some(snapshot)) => snapshot.captured_at.as_str() > period_start,
        (Some(_), None) => true,
        _ => false,
    }
}

fn parse_performance_period(value: Option<&str>) -> AppResult<PerformancePeriod> {
    match value.unwrap_or("month") {
        "month" => Ok(PerformancePeriod::Month),
        "year" => Ok(PerformancePeriod::Year),
        "since_inception" => Ok(PerformancePeriod::SinceInception),
        other => Err(AppError::bad_request(format!(
            "unsupported performance period: {other}"
        ))),
    }
}

fn period_start_utc(period: PerformancePeriod) -> Option<String> {
    let offset = chrono::FixedOffset::east_opt(8 * 60 * 60)?;
    let now = chrono::Utc::now().with_timezone(&offset);
    let start = match period {
        PerformancePeriod::Month => offset
            .with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
            .single()?,
        PerformancePeriod::Year => offset
            .with_ymd_and_hms(now.year(), 1, 1, 0, 0, 0)
            .single()?,
        PerformancePeriod::SinceInception => return None,
    };

    Some(start.with_timezone(&chrono::Utc).to_rfc3339())
}

impl PerformancePeriod {
    fn as_str(self) -> &'static str {
        match self {
            PerformancePeriod::Month => "month",
            PerformancePeriod::Year => "year",
            PerformancePeriod::SinceInception => "since_inception",
        }
    }
}

fn benchmark_definitions() -> [BenchmarkDefinition; 3] {
    [
        BenchmarkDefinition {
            key: "sp500",
            label: "S&P 500 ETF proxy",
            symbol: "SPY",
            currency: "USD",
        },
        BenchmarkDefinition {
            key: "hang_seng",
            label: "Hang Seng ETF proxy",
            symbol: "2800.HK",
            currency: "HKD",
        },
        BenchmarkDefinition {
            key: "sse",
            label: "SSE Composite",
            symbol: "000001.SS",
            currency: "CNY",
        },
    ]
}
