pub async fn list_cash_flows(
    pool: &SqlitePool,
    query: PortfolioCashFlowQuery,
) -> AppResult<Vec<PortfolioCashFlow>> {
    let period = parse_performance_period(query.period.as_deref())?;
    let period_start = period_start_utc(period);
    let rows = if let Some(period_start) = period_start {
        sqlx::query(
            r#"
            SELECT id, occurred_at, flow_type, currency, amount, fx_rate,
                   amount_base, note, source, created_at
            FROM portfolio_cash_flows
            WHERE occurred_at >= ?
            ORDER BY occurred_at DESC, id DESC
            "#,
        )
        .bind(period_start)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            r#"
            SELECT id, occurred_at, flow_type, currency, amount, fx_rate,
                   amount_base, note, source, created_at
            FROM portfolio_cash_flows
            ORDER BY occurred_at DESC, id DESC
            "#,
        )
        .fetch_all(pool)
        .await?
    };

    rows.into_iter().map(cash_flow_from_row).collect()
}

async fn record_automatic_trade_cash_flow(
    pool: &SqlitePool,
    captured_at: &str,
    source: &str,
    current_value_base: f64,
) -> AppResult<()> {
    if !should_record_trade_cash_flow(source) {
        return Ok(());
    }

    let previous_value_base = sqlx::query_scalar::<_, f64>(
        r#"
        SELECT total_market_value_base
        FROM portfolio_performance_snapshots
        WHERE captured_at < ?
        ORDER BY captured_at DESC, id DESC
        LIMIT 1
        "#,
    )
    .bind(captured_at)
    .fetch_optional(pool)
    .await?;
    let Some(previous_value_base) = previous_value_base else {
        return Ok(());
    };

    let amount_base = current_value_base - previous_value_base;
    if amount_base.abs() < 0.000001 {
        return Ok(());
    }

    let flow_type = if amount_base > 0.0 { "buy" } else { "sell" };
    sqlx::query(
        r#"
        INSERT INTO portfolio_cash_flows (
            id, occurred_at, flow_type, currency, amount, fx_rate, amount_base,
            note, source, created_at
        )
        VALUES (?, ?, ?, ?, ?, 1, ?, ?, ?, ?)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(captured_at)
    .bind(flow_type)
    .bind(BASE_CURRENCY)
    .bind(amount_base)
    .bind(amount_base)
    .bind(Some(format!("automatic trade adjustment from {source}")))
    .bind(source)
    .bind(now_iso())
    .execute(pool)
    .await?;

    Ok(())
}

fn cash_flow_from_row(row: sqlx::sqlite::SqliteRow) -> AppResult<PortfolioCashFlow> {
    Ok(PortfolioCashFlow {
        id: row.try_get("id")?,
        occurred_at: row.try_get("occurred_at")?,
        flow_type: row.try_get("flow_type")?,
        currency: row.try_get("currency")?,
        amount: row.try_get("amount")?,
        fx_rate: row.try_get("fx_rate")?,
        amount_base: row.try_get("amount_base")?,
        note: row.try_get("note")?,
        source: row.try_get("source")?,
        created_at: row.try_get("created_at")?,
    })
}

fn should_record_trade_cash_flow(source: &str) -> bool {
    matches!(
        source,
        "import_commit" | "draft_commit" | "position_update" | "position_delete"
    )
}
