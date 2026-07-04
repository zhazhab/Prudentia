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
struct BenchmarkSnapshotRow {
    captured_at: String,
    value_base: Option<f64>,
    stale: bool,
    error: Option<String>,
}

const PRICE_REFRESH_STATE_KEY: &str = "portfolio_prices";

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

    for benchmark in benchmark_definitions() {
        record_benchmark_snapshot(pool, market_data.clone(), &snapshot_id, &captured_at, benchmark)
            .await?;
    }

    tracing::info!(
        snapshot_id,
        source,
        total_market_value_base = summary.total_market_value_base,
        "portfolio performance snapshot recorded"
    );

    Ok(())
}

pub async fn portfolio_performance(
    pool: &SqlitePool,
    query: PortfolioPerformanceQuery,
) -> AppResult<PortfolioPerformanceResponse> {
    let period = parse_performance_period(query.period.as_deref())?;
    let period_start = period_start_utc(period);
    let snapshots = load_performance_snapshots(pool, period_start.as_deref()).await?;
    let portfolio = portfolio_metric(&snapshots);
    let series = portfolio_series(&snapshots);
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
) -> AppResult<()> {
    let currency = benchmark.currency.to_string();
    let mut price = None;
    let mut source = None;
    let mut fx_rate = None;
    let mut value_base = None;
    let stale;
    let error;

    match market_data.quote(benchmark.symbol).await {
        Ok(quote) => {
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

async fn load_benchmark_performance(
    pool: &SqlitePool,
    period: PerformancePeriod,
    period_start: Option<&str>,
) -> AppResult<Vec<BenchmarkPerformance>> {
    let mut benchmarks = Vec::new();
    for benchmark in benchmark_definitions() {
        let rows = load_benchmark_snapshots(pool, benchmark.key, period_start).await?;
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
    period_start: Option<&str>,
) -> AppResult<Vec<BenchmarkSnapshotRow>> {
    let rows = if let Some(period_start) = period_start {
        sqlx::query(
            r#"
            SELECT captured_at, value_base, stale, error
            FROM portfolio_benchmark_snapshots
            WHERE benchmark_key = ? AND captured_at >= ?
            ORDER BY captured_at ASC, id ASC
            "#,
        )
        .bind(benchmark_key)
        .bind(period_start)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            r#"
            SELECT captured_at, value_base, stale, error
            FROM portfolio_benchmark_snapshots
            WHERE benchmark_key = ?
            ORDER BY captured_at ASC, id ASC
            "#,
        )
        .bind(benchmark_key)
        .fetch_all(pool)
        .await?
    };

    rows.into_iter()
        .map(|row| {
            Ok(BenchmarkSnapshotRow {
                captured_at: row.try_get("captured_at")?,
                value_base: row.try_get("value_base")?,
                stale: row.try_get::<i64, _>("stale")? != 0,
                error: row.try_get("error")?,
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

fn portfolio_metric(snapshots: &[PerformanceSnapshotRow]) -> PortfolioPerformanceMetric {
    let start_snapshot = snapshots.first();
    let end_snapshot = snapshots.last();
    let start = start_snapshot.map(|snapshot| snapshot.total_market_value_base);
    let end = end_snapshot.map(|snapshot| snapshot.total_market_value_base);
    let profit_loss = start.zip(end).map(|(start, end)| end - start);
    let return_pct = start
        .zip(end)
        .and_then(|(start, end)| percentage_return(start, end));
    let annualized_return_pct = start_snapshot
        .zip(end_snapshot)
        .and_then(|(start_snapshot, end_snapshot)| {
            annualized_return(
                start_snapshot.total_market_value_base,
                end_snapshot.total_market_value_base,
                &start_snapshot.captured_at,
                &end_snapshot.captured_at,
            )
        });

    PortfolioPerformanceMetric {
        start_value_base: start,
        end_value_base: end,
        profit_loss_base: profit_loss,
        return_pct,
        annualized_return_pct,
    }
}

fn portfolio_series(snapshots: &[PerformanceSnapshotRow]) -> Vec<PortfolioPerformancePoint> {
    let start_snapshot = snapshots.first();
    snapshots
        .iter()
        .map(|snapshot| PortfolioPerformancePoint {
            captured_at: snapshot.captured_at.clone(),
            value_base: snapshot.total_market_value_base,
            profit_loss_base: start_snapshot
                .map(|start| snapshot.total_market_value_base - start.total_market_value_base),
            return_pct: start_snapshot.and_then(|start| {
                percentage_return(start.total_market_value_base, snapshot.total_market_value_base)
            }),
            annualized_return_pct: start_snapshot.and_then(|start| {
                annualized_return(
                    start.total_market_value_base,
                    snapshot.total_market_value_base,
                    &start.captured_at,
                    &snapshot.captured_at,
                )
            }),
        })
        .collect()
}

fn percentage_return(start: f64, end: f64) -> Option<f64> {
    if start.abs() < f64::EPSILON {
        return None;
    }
    Some(end / start - 1.0)
}

fn annualized_return(start: f64, end: f64, start_at: &str, end_at: &str) -> Option<f64> {
    let return_pct = percentage_return(start, end)?;
    if return_pct.abs() < f64::EPSILON {
        return Some(0.0);
    }

    let start_at = chrono::DateTime::parse_from_rfc3339(start_at).ok()?;
    let end_at = chrono::DateTime::parse_from_rfc3339(end_at).ok()?;
    let elapsed_seconds = end_at.signed_duration_since(start_at).num_seconds();
    if elapsed_seconds <= 0 {
        return None;
    }

    let elapsed_days = elapsed_seconds as f64 / 86_400.0;
    let ratio = end / start;
    if ratio <= 0.0 {
        return None;
    }

    Some(ratio.powf(365.25 / elapsed_days) - 1.0)
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
            label: "SSE Composite ETF proxy",
            symbol: "510210.SS",
            currency: "CNY",
        },
    ]
}
