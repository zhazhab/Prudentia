async fn performance_window(
    pool: &SqlitePool,
    period_start: Option<String>,
) -> AppResult<PerformanceWindow> {
    let reset_after = latest_empty_portfolio_reset(pool, period_start.as_deref()).await?;
    Ok(PerformanceWindow {
        period_start,
        reset_after,
    })
}

async fn latest_empty_portfolio_reset(
    pool: &SqlitePool,
    period_start: Option<&str>,
) -> AppResult<Option<String>> {
    let row = if let Some(period_start) = period_start {
        sqlx::query(
            r#"
            SELECT captured_at
            FROM portfolio_performance_snapshots
            WHERE source = 'position_delete'
              AND total_market_value_base = 0
              AND captured_at >= ?
            ORDER BY captured_at DESC, id DESC
            LIMIT 1
            "#,
        )
        .bind(period_start)
        .fetch_optional(pool)
        .await?
    } else {
        sqlx::query(
            r#"
            SELECT captured_at
            FROM portfolio_performance_snapshots
            WHERE source = 'position_delete'
              AND total_market_value_base = 0
            ORDER BY captured_at DESC, id DESC
            LIMIT 1
            "#,
        )
        .fetch_optional(pool)
        .await?
    };

    Ok(row.map(|row| row.try_get("captured_at")).transpose()?)
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
                let interval_flow = cash_flow_sum_between(
                    cash_flows,
                    &previous_snapshot.captured_at,
                    &snapshot.captured_at,
                );
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

impl PerformanceWindow {
    fn start_boundary(&self) -> Option<&str> {
        self.reset_after
            .as_deref()
            .or(self.period_start.as_deref())
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
