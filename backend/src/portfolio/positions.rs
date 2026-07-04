pub async fn list_positions(pool: &SqlitePool) -> AppResult<Vec<PortfolioPosition>> {
    let rows = sqlx::query(
        r#"
        SELECT symbol, name, asset_type, quantity, average_cost, currency, account, market,
               sector, notes, last_price, market_value, unrealized_pnl, weight,
               price_updated_at, price_stale, updated_at
        FROM portfolio_positions
        ORDER BY market_value DESC, symbol ASC
        "#,
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter().map(position_from_db_row).collect()
}

pub async fn update_position(
    pool: &SqlitePool,
    market_data: Arc<dyn MarketDataProvider>,
    symbol: &str,
    request: UpdatePortfolioPositionRequest,
) -> AppResult<PortfolioPosition> {
    let mut position = get_position(pool, symbol).await?;

    if let Some(name) = request.name.and_then(clean_string) {
        position.name = name;
    }
    if let Some(quantity) = request.quantity {
        if quantity <= 0.0 {
            return Err(AppError::bad_request("quantity must be greater than 0"));
        }
        position.quantity = quantity;
    }
    if let Some(average_cost) = request.average_cost {
        if average_cost < 0.0 {
            return Err(AppError::bad_request("average_cost must be non-negative"));
        }
        position.average_cost = average_cost;
    }
    if let Some(currency) = request.currency.and_then(clean_string) {
        position.currency = currency.to_ascii_uppercase();
    }
    if let Some(market) = request.market.and_then(clean_string) {
        position.market = Some(normalize_market(&market));
    }
    if request.account.is_some() {
        position.account = request.account.and_then(clean_string);
    }
    if request.sector.is_some() {
        position.sector = request.sector.and_then(clean_string);
    }
    if request.notes.is_some() {
        position.notes = request.notes.and_then(clean_string);
    }

    if let Some(imported_market_value) = request.imported_market_value {
        if imported_market_value < 0.0 {
            return Err(AppError::bad_request(
                "imported_market_value must be non-negative",
            ));
        }
        position.last_price = Some(ratio(imported_market_value, position.quantity));
        position.market_value = imported_market_value;
    } else {
        let last_price = position.last_price.unwrap_or(position.average_cost);
        position.last_price = Some(last_price);
        position.market_value = last_price * position.quantity;
    }

    position.unrealized_pnl = position.market_value - position.average_cost * position.quantity;
    position.price_stale = true;
    position.updated_at = now_iso();

    upsert_position(pool, &position).await?;
    recompute_weights_with_fx(pool, market_data.clone()).await?;
    record_portfolio_performance_snapshot(pool, market_data, "position_update").await?;
    get_position(pool, &position.symbol).await
}

pub async fn delete_position(
    pool: &SqlitePool,
    market_data: Arc<dyn MarketDataProvider>,
    symbol: &str,
) -> AppResult<()> {
    let normalized = symbol.trim().to_ascii_uppercase();
    let result = sqlx::query("DELETE FROM portfolio_positions WHERE symbol = ?")
        .bind(&normalized)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::not_found("position not found"));
    }
    recompute_weights_with_fx(pool, market_data.clone()).await?;
    record_portfolio_performance_snapshot(pool, market_data, "position_delete").await?;
    Ok(())
}

pub async fn summary(pool: &SqlitePool) -> AppResult<PortfolioSummary> {
    let positions = list_positions(pool).await?;
    let fx_rates = load_fx_rates(pool).await?;
    summary_from_positions(&positions, &fx_rates)
}

pub async fn summary_with_fx(
    pool: &SqlitePool,
    market_data: Arc<dyn MarketDataProvider>,
) -> AppResult<PortfolioSummary> {
    let positions = list_positions(pool).await?;
    refresh_fx_rates_for_positions(pool, market_data, &positions).await?;
    let fx_rates = load_fx_rates(pool).await?;
    summary_from_positions(&positions, &fx_rates)
}

fn summary_from_positions(
    positions: &[PortfolioPosition],
    fx_rates: &[PortfolioFxRate],
) -> AppResult<PortfolioSummary> {
    let total_market_value = positions
        .iter()
        .map(|position| position.market_value)
        .sum::<f64>();
    let total_cost = positions
        .iter()
        .map(|position| position.quantity * position.average_cost)
        .sum::<f64>();
    let total_unrealized_pnl = positions
        .iter()
        .map(|position| position.unrealized_pnl)
        .sum::<f64>();
    let price_stale_count = positions
        .iter()
        .filter(|position| position.price_stale)
        .count();

    let total_market_value_base = positions
        .iter()
        .map(|position| position.market_value * fx_rate_for(&position.currency, fx_rates))
        .sum::<f64>();
    let total_cost_base = positions
        .iter()
        .map(|position| {
            position.quantity * position.average_cost * fx_rate_for(&position.currency, fx_rates)
        })
        .sum::<f64>();
    let total_unrealized_pnl_base = total_market_value_base - total_cost_base;

    let top_positions = positions
        .iter()
        .take(5)
        .map(|position| WeightSlice {
            label: position.symbol.clone(),
            value: position.market_value,
            weight: ratio(
                position.market_value * fx_rate_for(&position.currency, fx_rates),
                total_market_value_base,
            ),
        })
        .collect();

    let mut sectors_by_value: HashMap<String, f64> = HashMap::new();
    for position in positions {
        let label = position
            .sector
            .clone()
            .unwrap_or_else(|| "Unclassified".to_string());
        *sectors_by_value.entry(label).or_default() += position.market_value;
    }
    let mut sectors = sectors_by_value
        .into_iter()
        .map(|(label, value)| WeightSlice {
            label,
            value,
            weight: ratio(value, total_market_value),
        })
        .collect::<Vec<_>>();
    sectors.sort_by(|a, b| b.value.total_cmp(&a.value));

    let mut market_values: HashMap<(String, String), (f64, f64, f64)> = HashMap::new();
    for position in positions {
        let market = position.market.clone().unwrap_or_else(|| {
            infer_market(&position.symbol).unwrap_or_else(|| "Other".to_string())
        });
        let entry = market_values
            .entry((market, position.currency.clone()))
            .or_default();
        entry.0 += position.market_value;
        entry.1 += position.quantity * position.average_cost;
        entry.2 += position.unrealized_pnl;
    }
    let mut market_groups = market_values
        .into_iter()
        .map(
            |((market, currency), (market_value, cost, unrealized_pnl))| {
                let rate = fx_rate_for(&currency, fx_rates);
                let market_value_base = market_value * rate;
                MarketValueGroup {
                    market,
                    currency,
                    market_value,
                    cost,
                    unrealized_pnl,
                    market_value_base,
                    weight: ratio(market_value_base, total_market_value_base),
                }
            },
        )
        .collect::<Vec<_>>();
    market_groups.sort_by(|a, b| b.market_value_base.total_cmp(&a.market_value_base));
    let fx_stale_count = fx_rates.iter().filter(|rate| rate.stale).count();

    Ok(PortfolioSummary {
        total_market_value,
        total_cost,
        total_unrealized_pnl,
        positions_count: positions.len(),
        price_stale_count,
        top_positions,
        sectors,
        market_groups,
        base_currency: BASE_CURRENCY.to_string(),
        total_market_value_base,
        total_cost_base,
        total_unrealized_pnl_base,
        fx_rates: fx_rates.to_vec(),
        fx_stale_count,
        updated_at: now_iso(),
    })
}

pub async fn refresh_prices(
    pool: &SqlitePool,
    market_data: Arc<dyn MarketDataProvider>,
) -> AppResult<PriceRefreshResult> {
    let positions = list_positions(pool).await?;
    let mut refreshed = 0;
    let mut failed = 0;
    let mut failures = Vec::new();

    for position in positions {
        match market_data.quote(&position.symbol).await {
            Ok(quote) => {
                if quote.source.trim().eq_ignore_ascii_case("mock") {
                    failed += 1;
                    failures.push(format!(
                        "{}: mock quote provider does not update portfolio prices",
                        position.symbol
                    ));
                    mark_position_price_stale(pool, &position).await?;
                    continue;
                }
                let market_value = quote.price * position.quantity;
                let unrealized_pnl = market_value - (position.average_cost * position.quantity);
                sqlx::query(
                    r#"
                    UPDATE portfolio_positions
                    SET last_price = ?, market_value = ?, unrealized_pnl = ?,
                        price_updated_at = ?, price_stale = 0, updated_at = ?
                    WHERE symbol = ?
                    "#,
                )
                .bind(quote.price)
                .bind(market_value)
                .bind(unrealized_pnl)
                .bind(quote.updated_at)
                .bind(now_iso())
                .bind(&position.symbol)
                .execute(pool)
                .await?;
                refreshed += 1;
            }
            Err(error) => {
                failed += 1;
                failures.push(format!("{}: {}", position.symbol, error));
                mark_position_price_stale(pool, &position).await?;
            }
        }
    }

    if let Err(error) = recompute_weights_with_fx(pool, market_data.clone()).await {
        tracing::warn!(error = ?error, "falling back to native portfolio weights after FX refresh failed");
        recompute_weights(pool).await?;
    }
    record_portfolio_performance_snapshot(pool, market_data, "price_refresh").await?;

    Ok(PriceRefreshResult {
        refreshed,
        failed,
        failures,
        positions: list_positions(pool).await?,
    })
}

async fn mark_position_price_stale(pool: &SqlitePool, position: &PortfolioPosition) -> AppResult<()> {
    if let Some(last_price) = extract_visible_last_price(position.notes.as_deref())
        .as_deref()
        .and_then(|value| parse_non_negative_f64(value, "last_price").ok())
    {
        let market_value = last_price * position.quantity;
        let unrealized_pnl = market_value - position.average_cost * position.quantity;
        sqlx::query(
            r#"
            UPDATE portfolio_positions
            SET last_price = ?, market_value = ?, unrealized_pnl = ?,
                price_stale = 1, updated_at = ?
            WHERE symbol = ?
            "#,
        )
        .bind(last_price)
        .bind(market_value)
        .bind(unrealized_pnl)
        .bind(now_iso())
        .bind(&position.symbol)
        .execute(pool)
        .await?;
        return Ok(());
    }

    sqlx::query(
        r#"
        UPDATE portfolio_positions
        SET price_stale = 1, updated_at = ?
        WHERE symbol = ?
        "#,
    )
    .bind(now_iso())
    .bind(&position.symbol)
    .execute(pool)
    .await?;
    Ok(())
}

async fn upsert_position(pool: &SqlitePool, position: &PortfolioPosition) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO portfolio_positions (
            symbol, name, asset_type, quantity, average_cost, currency, account, market,
            sector, notes, last_price, market_value, unrealized_pnl, weight,
            price_updated_at, price_stale, updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(symbol) DO UPDATE SET
            name = excluded.name,
            asset_type = excluded.asset_type,
            quantity = excluded.quantity,
            average_cost = excluded.average_cost,
            currency = excluded.currency,
            account = excluded.account,
            market = excluded.market,
            sector = excluded.sector,
            notes = excluded.notes,
            last_price = excluded.last_price,
            market_value = excluded.market_value,
            unrealized_pnl = excluded.unrealized_pnl,
            price_stale = excluded.price_stale,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(&position.symbol)
    .bind(&position.name)
    .bind(&position.asset_type)
    .bind(position.quantity)
    .bind(position.average_cost)
    .bind(&position.currency)
    .bind(&position.account)
    .bind(&position.market)
    .bind(&position.sector)
    .bind(&position.notes)
    .bind(position.last_price)
    .bind(position.market_value)
    .bind(position.unrealized_pnl)
    .bind(position.weight)
    .bind(&position.price_updated_at)
    .bind(position.price_stale)
    .bind(&position.updated_at)
    .execute(pool)
    .await?;

    Ok(())
}

async fn recompute_weights(pool: &SqlitePool) -> AppResult<()> {
    let positions = list_positions(pool).await?;
    let total_market_value = positions
        .iter()
        .map(|position| position.market_value)
        .sum::<f64>();

    for position in positions {
        sqlx::query("UPDATE portfolio_positions SET weight = ? WHERE symbol = ?")
            .bind(ratio(position.market_value, total_market_value))
            .bind(position.symbol)
            .execute(pool)
            .await?;
    }

    Ok(())
}

async fn recompute_weights_with_fx(
    pool: &SqlitePool,
    market_data: Arc<dyn MarketDataProvider>,
) -> AppResult<()> {
    let positions = list_positions(pool).await?;
    refresh_fx_rates_for_positions(pool, market_data, &positions).await?;
    let fx_rates = load_fx_rates(pool).await?;
    let total_market_value_base = positions
        .iter()
        .map(|position| position.market_value * fx_rate_for(&position.currency, &fx_rates))
        .sum::<f64>();

    for position in positions {
        let base_value = position.market_value * fx_rate_for(&position.currency, &fx_rates);
        sqlx::query("UPDATE portfolio_positions SET weight = ? WHERE symbol = ?")
            .bind(ratio(base_value, total_market_value_base))
            .bind(position.symbol)
            .execute(pool)
            .await?;
    }

    Ok(())
}

async fn get_position(pool: &SqlitePool, symbol: &str) -> AppResult<PortfolioPosition> {
    let normalized = symbol.trim().to_ascii_uppercase();
    let row = sqlx::query(
        r#"
        SELECT symbol, name, asset_type, quantity, average_cost, currency, account, market,
               sector, notes, last_price, market_value, unrealized_pnl, weight,
               price_updated_at, price_stale, updated_at
        FROM portfolio_positions
        WHERE symbol = ?
        "#,
    )
    .bind(normalized)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::not_found("position not found"))?;

    position_from_db_row(row)
}

async fn refresh_fx_rates_for_positions(
    pool: &SqlitePool,
    market_data: Arc<dyn MarketDataProvider>,
    positions: &[PortfolioPosition],
) -> AppResult<()> {
    let mut currencies = positions
        .iter()
        .map(|position| position.currency.to_ascii_uppercase())
        .filter(|currency| !currency.trim().is_empty())
        .collect::<Vec<_>>();
    currencies.sort();
    currencies.dedup();

    for currency in currencies {
        refresh_fx_rate(pool, market_data.clone(), &currency, BASE_CURRENCY).await?;
    }

    Ok(())
}

async fn refresh_fx_rate(
    pool: &SqlitePool,
    market_data: Arc<dyn MarketDataProvider>,
    from_currency: &str,
    to_currency: &str,
) -> AppResult<()> {
    let from_currency = from_currency.trim().to_ascii_uppercase();
    let to_currency = to_currency.trim().to_ascii_uppercase();
    if from_currency.is_empty() || to_currency.is_empty() {
        return Ok(());
    }

    if from_currency == to_currency {
        upsert_fx_rate(
            pool,
            &PortfolioFxRate {
                from_currency,
                to_currency,
                rate: 1.0,
                source: "identity".to_string(),
                updated_at: now_iso(),
                stale: false,
            },
        )
        .await?;
        return Ok(());
    }

    match market_data
        .exchange_rate(&from_currency, &to_currency)
        .await
    {
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
        }
        Err(error) => {
            if mark_fx_rate_stale(pool, &from_currency, &to_currency).await? {
                tracing::warn!(
                    from_currency,
                    to_currency,
                    error = ?error,
                    "using stale portfolio FX rate"
                );
            } else {
                return Err(AppError::bad_request(format!(
                    "missing FX rate for {from_currency}/{to_currency}: {error}"
                )));
            }
        }
    }

    Ok(())
}

async fn upsert_fx_rate(pool: &SqlitePool, rate: &PortfolioFxRate) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO portfolio_fx_rates (
            from_currency, to_currency, rate, source, updated_at, stale
        )
        VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT(from_currency, to_currency) DO UPDATE SET
            rate = excluded.rate,
            source = excluded.source,
            updated_at = excluded.updated_at,
            stale = excluded.stale
        "#,
    )
    .bind(&rate.from_currency)
    .bind(&rate.to_currency)
    .bind(rate.rate)
    .bind(&rate.source)
    .bind(&rate.updated_at)
    .bind(rate.stale)
    .execute(pool)
    .await?;

    Ok(())
}

async fn mark_fx_rate_stale(
    pool: &SqlitePool,
    from_currency: &str,
    to_currency: &str,
) -> AppResult<bool> {
    let result = sqlx::query(
        r#"
        UPDATE portfolio_fx_rates
        SET stale = 1
        WHERE from_currency = ? AND to_currency = ?
        "#,
    )
    .bind(from_currency)
    .bind(to_currency)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

async fn load_fx_rates(pool: &SqlitePool) -> AppResult<Vec<PortfolioFxRate>> {
    let rows = sqlx::query(
        r#"
        SELECT from_currency, to_currency, rate, source, updated_at, stale
        FROM portfolio_fx_rates
        WHERE to_currency = ?
        ORDER BY from_currency ASC
        "#,
    )
    .bind(BASE_CURRENCY)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|row| {
            Ok(PortfolioFxRate {
                from_currency: row.try_get("from_currency")?,
                to_currency: row.try_get("to_currency")?,
                rate: row.try_get("rate")?,
                source: row.try_get("source")?,
                updated_at: row.try_get("updated_at")?,
                stale: row.try_get::<i64, _>("stale")? != 0,
            })
        })
        .collect()
}

fn fx_rate_for(currency: &str, fx_rates: &[PortfolioFxRate]) -> f64 {
    if currency.eq_ignore_ascii_case(BASE_CURRENCY) {
        return 1.0;
    }

    fx_rates
        .iter()
        .find(|rate| {
            rate.from_currency.eq_ignore_ascii_case(currency)
                && rate.to_currency.eq_ignore_ascii_case(BASE_CURRENCY)
        })
        .map(|rate| rate.rate)
        .unwrap_or(0.0)
}
