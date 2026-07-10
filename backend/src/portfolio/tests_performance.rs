#[tokio::test]
async fn portfolio_return_uses_time_weighted_cash_flow_adjustment() {
    let pool = migrated_pool().await;

    seed_performance_snapshot(&pool, "snap-1", "2026-07-01T00:00:00+00:00", 1000.0).await;
    seed_performance_snapshot(&pool, "snap-2", "2026-07-02T00:00:00+00:00", 1650.0).await;
    seed_performance_snapshot(&pool, "snap-3", "2026-07-03T00:00:00+00:00", 1815.0).await;
    sqlx::query(
        r#"
        INSERT INTO portfolio_cash_flows (
            id, occurred_at, flow_type, currency, amount, fx_rate, amount_base,
            note, source, created_at
        )
        VALUES ('flow-1', '2026-07-01T12:00:00+00:00', 'buy', 'CNY', 500, 1, 500,
            'test buy adjustment', 'draft_commit', '2026-07-01T12:00:00+00:00')
        "#,
    )
    .execute(&pool)
    .await
    .expect("insert cash flow");

    let performance = portfolio_performance(
        &pool,
        PortfolioPerformanceQuery {
            period: Some("since_inception".to_string()),
        },
    )
    .await
    .expect("performance");

    assert_eq!(performance.portfolio.net_cash_flow_base, 500.0);
    assert_eq!(performance.portfolio.simple_return_pct, Some(0.815));
    assert!(
        (performance.portfolio.return_pct.expect("twr return") - 0.265).abs() < 0.000001
    );
    assert_eq!(performance.portfolio.profit_loss_base, Some(315.0));
    assert_eq!(performance.portfolio.return_method, "time_weighted");
    assert_eq!(performance.series[1].net_cash_flow_base, 500.0);
    assert!((performance.series[1].return_pct.expect("point twr") - 0.15).abs() < 0.000001);
    assert!((performance.series[2].return_pct.expect("point twr") - 0.265).abs() < 0.000001);
}

#[tokio::test]
async fn position_change_snapshot_records_automatic_trade_cash_flow() {
    let pool = migrated_pool().await;
    seed_performance_snapshot(&pool, "snap-1", "2026-07-01T00:00:00+00:00", 1000.0).await;
    seed_position_value(&pool, "0700.HK", "Tencent", 1500.0).await;

    record_portfolio_performance_snapshot(&pool, Arc::new(MockMarketDataProvider), "draft_commit")
        .await
        .expect("record snapshot");

    let flows = list_cash_flows(
        &pool,
        PortfolioCashFlowQuery {
            period: Some("since_inception".to_string()),
        },
    )
    .await
    .expect("list cash flows");

    assert_eq!(flows.len(), 1);
    assert_eq!(flows[0].flow_type, "buy");
    assert_eq!(flows[0].source, "draft_commit");
    assert_eq!(flows[0].amount_base, 500.0);

    let performance = portfolio_performance(
        &pool,
        PortfolioPerformanceQuery {
            period: Some("since_inception".to_string()),
        },
    )
    .await
    .expect("performance");

    assert_eq!(performance.portfolio.net_cash_flow_base, 500.0);
    assert_eq!(performance.portfolio.simple_return_pct, Some(0.5));
    assert!((performance.portfolio.return_pct.expect("twr return") - 0.0).abs() < 0.000001);
    assert_eq!(performance.portfolio.profit_loss_base, Some(0.0));
}

#[tokio::test]
async fn price_refresh_snapshot_does_not_record_trade_cash_flow() {
    let pool = migrated_pool().await;
    seed_performance_snapshot(&pool, "snap-1", "2026-07-01T00:00:00+00:00", 1000.0).await;
    seed_position_value(&pool, "0700.HK", "Tencent", 1500.0).await;

    record_portfolio_performance_snapshot(
        &pool,
        Arc::new(MockMarketDataProvider),
        "price_refresh",
    )
    .await
    .expect("record snapshot");

    let trade_adjustment_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM portfolio_cash_flows")
            .fetch_one(&pool)
            .await
            .expect("count trade adjustments");
    assert_eq!(trade_adjustment_count, 0);

    let performance = portfolio_performance(
        &pool,
        PortfolioPerformanceQuery {
            period: Some("since_inception".to_string()),
        },
    )
    .await
    .expect("performance");

    assert_eq!(performance.portfolio.net_cash_flow_base, 0.0);
    assert_eq!(performance.portfolio.simple_return_pct, Some(0.5));
    assert_eq!(performance.portfolio.return_pct, Some(0.5));
    assert_eq!(performance.portfolio.profit_loss_base, Some(500.0));
}

#[tokio::test]
async fn performance_snapshot_marks_mock_benchmarks_unavailable() {
    let pool = migrated_pool().await;

    record_portfolio_performance_snapshot(
        &pool,
        Arc::new(MockMarketDataProvider),
        "price_refresh",
    )
    .await
    .expect("record snapshot");

    let performance = portfolio_performance(
        &pool,
        PortfolioPerformanceQuery {
            period: Some("since_inception".to_string()),
        },
    )
    .await
    .expect("performance");

    assert_eq!(performance.benchmarks.len(), 3);
    for benchmark in performance.benchmarks {
        assert!(
            !benchmark.available,
            "{} should not treat mock quotes as available",
            benchmark.key
        );
        assert!(
            benchmark.stale,
            "{} should mark mock quotes stale",
            benchmark.key
        );
        assert_eq!(benchmark.start_value_base, None);
        assert_eq!(benchmark.end_value_base, None);
        assert_eq!(benchmark.return_pct, None);
        assert_eq!(benchmark.series.len(), 1);
        assert_eq!(benchmark.series[0].value_base, None);
        assert!(benchmark.series[0].stale);
        assert!(benchmark.series[0]
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("mock"));
    }
}

async fn seed_performance_snapshot(pool: &SqlitePool, id: &str, captured_at: &str, value: f64) {
    sqlx::query(
        r#"
        INSERT INTO portfolio_performance_snapshots (
            id, captured_at, source, base_currency, total_market_value_base,
            total_cost_base, total_unrealized_pnl_base
        )
        VALUES (?, ?, 'price_refresh', 'CNY', ?, ?, 0)
        "#,
    )
    .bind(id)
    .bind(captured_at)
    .bind(value)
    .bind(value)
    .execute(pool)
    .await
    .expect("insert portfolio snapshot");
}

async fn seed_position_value(pool: &SqlitePool, symbol: &str, name: &str, value: f64) {
    sqlx::query(
        r#"
        INSERT INTO portfolio_positions (
            symbol, name, asset_type, quantity, average_cost, currency, account,
            market, sector, notes, last_price, market_value, unrealized_pnl,
            weight, price_updated_at, price_stale, updated_at
        )
        VALUES (?, ?, 'stock', 1, ?, 'CNY', NULL, 'CN', NULL, NULL, ?, ?, 0, 1, NULL, 0,
            '2026-07-01T00:00:00+00:00')
        "#,
    )
    .bind(symbol)
    .bind(name)
    .bind(value)
    .bind(value)
    .bind(value)
    .execute(pool)
    .await
    .expect("insert position");
}

#[test]
fn sse_benchmark_uses_official_composite_index() {
    let sse = benchmark_definitions()
        .into_iter()
        .find(|benchmark| benchmark.key == "sse")
        .expect("sse benchmark");

    assert_eq!(sse.symbol, "000001.SS");
    assert_eq!(sse.label, "SSE Composite");
}

#[tokio::test]
async fn benchmarks_are_recorded_only_during_price_refresh_cycle() {
    let pool = migrated_pool().await;

    record_portfolio_performance_snapshot(
        &pool,
        Arc::new(BatchOnlyMarketDataProvider {
            batch_calls: StdArc::new(AtomicUsize::new(0)),
            single_calls: StdArc::new(AtomicUsize::new(0)),
        }),
        "import_commit",
    )
    .await
    .expect("record import snapshot");

    let benchmark_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM portfolio_benchmark_snapshots")
            .fetch_one(&pool)
            .await
            .expect("count import benchmark snapshots");
    assert_eq!(benchmark_count, 0);

    record_portfolio_performance_snapshot(
        &pool,
        Arc::new(BatchOnlyMarketDataProvider {
            batch_calls: StdArc::new(AtomicUsize::new(0)),
            single_calls: StdArc::new(AtomicUsize::new(0)),
        }),
        "price_refresh",
    )
    .await
    .expect("record price refresh snapshot");

    let benchmark_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM portfolio_benchmark_snapshots")
            .fetch_one(&pool)
            .await
            .expect("count price refresh benchmark snapshots");
    assert_eq!(benchmark_count, 3);
}

#[tokio::test]
async fn performance_ignores_legacy_mock_benchmark_values() {
    let pool = migrated_pool().await;
    let snapshot_id = Uuid::new_v4().to_string();
    let captured_at = "2026-07-05T00:25:14+00:00";

    sqlx::query(
        r#"
        INSERT INTO portfolio_performance_snapshots (
            id, captured_at, source, base_currency, total_market_value_base,
            total_cost_base, total_unrealized_pnl_base
        )
        VALUES (?, ?, 'price_refresh', 'CNY', 1000, 900, 100)
        "#,
    )
    .bind(&snapshot_id)
    .bind(captured_at)
    .execute(&pool)
    .await
    .expect("insert portfolio snapshot");

    sqlx::query(
        r#"
        INSERT INTO portfolio_benchmark_snapshots (
            id, snapshot_id, benchmark_key, label, symbol, currency, price,
            fx_rate, value_base, source, stale, error, captured_at
        )
        VALUES (?, ?, 'sp500', 'S&P 500 ETF proxy', 'SPY', 'USD', 273.4,
            7.2, 1968.48, 'mock', 0, NULL, ?)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&snapshot_id)
    .bind(captured_at)
    .execute(&pool)
    .await
    .expect("insert mock benchmark snapshot");

    let performance = portfolio_performance(
        &pool,
        PortfolioPerformanceQuery {
            period: Some("since_inception".to_string()),
        },
    )
    .await
    .expect("performance");
    let sp500 = performance
        .benchmarks
        .iter()
        .find(|benchmark| benchmark.key == "sp500")
        .expect("sp500 benchmark");

    assert!(!sp500.available);
    assert!(sp500.stale);
    assert_eq!(sp500.start_value_base, None);
    assert_eq!(sp500.series.len(), 1);
    assert_eq!(sp500.series[0].value_base, None);
    assert!(sp500.series[0]
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("mock"));
}

#[tokio::test]
async fn performance_ignores_legacy_benchmark_rows_for_replaced_symbols() {
    let pool = migrated_pool().await;
    let snapshot_id = Uuid::new_v4().to_string();
    let captured_at = "2026-07-09T01:15:23+00:00";

    sqlx::query(
        r#"
        INSERT INTO portfolio_performance_snapshots (
            id, captured_at, source, base_currency, total_market_value_base,
            total_cost_base, total_unrealized_pnl_base
        )
        VALUES (?, ?, 'price_refresh', 'CNY', 1000, 900, 100)
        "#,
    )
    .bind(&snapshot_id)
    .bind(captured_at)
    .execute(&pool)
    .await
    .expect("insert portfolio snapshot");

    sqlx::query(
        r#"
        INSERT INTO portfolio_benchmark_snapshots (
            id, snapshot_id, benchmark_key, label, symbol, currency, price,
            fx_rate, value_base, source, stale, error, captured_at
        )
        VALUES (?, ?, 'sse', 'SSE Composite ETF proxy', '510210.SS', 'CNY', 1.002,
            1.0, 1.002, 'yahoo', 0, NULL, ?)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&snapshot_id)
    .bind(captured_at)
    .execute(&pool)
    .await
    .expect("insert legacy sse benchmark snapshot");

    let performance = portfolio_performance(
        &pool,
        PortfolioPerformanceQuery {
            period: Some("since_inception".to_string()),
        },
    )
    .await
    .expect("performance");
    let sse = performance
        .benchmarks
        .iter()
        .find(|benchmark| benchmark.key == "sse")
        .expect("sse benchmark");

    assert_eq!(sse.symbol, "000001.SS");
    assert!(!sse.available);
    assert!(sse.series.is_empty());
}
