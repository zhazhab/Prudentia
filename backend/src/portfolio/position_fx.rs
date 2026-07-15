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
