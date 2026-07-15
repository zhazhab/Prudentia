#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRecord {
    pub side: String,
    pub symbol: String,
    pub quantity: f64,
    pub price: f64,
    pub currency: String,
    pub occurred_at: String,
    #[serde(default)]
    pub fees: f64,
    #[serde(default)]
    pub account: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub fx_rate: Option<f64>,
    #[serde(default)]
    pub fx_source: Option<String>,
    #[serde(default)]
    pub corrects_trade_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeReceipt {
    pub trade_id: String,
    pub symbol: String,
    pub impacts_portfolio: bool,
    pub baseline_effective_at: String,
    pub amount_base: Option<f64>,
    pub fx_rate: Option<f64>,
    pub fx_source: Option<String>,
    pub position: Option<PortfolioPosition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioTradeEvent {
    pub id: String,
    pub event_kind: String,
    pub symbol: String,
    pub side: Option<String>,
    pub quantity: Option<f64>,
    pub price: Option<f64>,
    pub fees: Option<f64>,
    pub currency: String,
    pub occurred_at: String,
    pub fx_rate: Option<f64>,
    pub fx_source: Option<String>,
    pub amount_base: Option<f64>,
    pub impacts_portfolio: bool,
    pub reverses_trade_id: Option<String>,
    pub correction_of_trade_id: Option<String>,
    pub account: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
struct PositionBaseline {
    symbol: String,
    effective_at: String,
    name: String,
    asset_type: String,
    quantity: f64,
    average_cost: f64,
    currency: String,
    account: Option<String>,
    market: Option<String>,
    sector: Option<String>,
    notes: Option<String>,
    last_price: Option<f64>,
}

pub async fn prepare_trade_record(
    pool: &SqlitePool,
    market_data: Arc<dyn MarketDataProvider>,
    mut trade: TradeRecord,
) -> AppResult<TradeRecord> {
    normalize_trade(&mut trade)?;
    if trade.fx_rate.is_none() {
        let rate = historical_fx_rate(
            pool,
            market_data,
            &trade.currency,
            BASE_CURRENCY,
            trade_date(&trade.occurred_at)?,
        )
        .await?;
        trade.fx_rate = Some(rate.rate);
        trade.fx_source = Some(rate.source);
    }
    Ok(trade)
}

pub async fn record_trade(
    pool: &SqlitePool,
    market_data: Arc<dyn MarketDataProvider>,
    mut trade: TradeRecord,
    action_id: Option<&str>,
) -> AppResult<TradeReceipt> {
    normalize_trade(&mut trade)?;
    let baseline = ensure_position_baseline(pool, &trade).await?;
    let impacts_portfolio = timestamp_after(&trade.occurred_at, &baseline.effective_at)?;
    let (fx_rate, fx_source) = if impacts_portfolio {
        if let Some(rate) = trade.fx_rate {
            if rate <= 0.0 {
                return Err(AppError::bad_request("trade fx_rate must be positive"));
            }
            (Some(rate), trade.fx_source.clone().or_else(|| Some("manual".to_string())))
        } else {
            let rate = historical_fx_rate(
                pool,
                market_data.clone(),
                &trade.currency,
                BASE_CURRENCY,
                trade_date(&trade.occurred_at)?,
            )
            .await?;
            (Some(rate.rate), Some(rate.source))
        }
    } else {
        (trade.fx_rate, trade.fx_source.clone())
    };

    let excluded = trade.corrects_trade_id.as_deref();
    let (available_quantity, _) = projected_position(pool, &baseline, excluded).await?;
    if trade.side == "sell" && trade.quantity > available_quantity + 0.0000001 && impacts_portfolio {
        return Err(AppError::bad_request(format!(
            "sell quantity {} exceeds available quantity {}",
            trade.quantity, available_quantity
        )));
    }

    if let Some(original_id) = excluded {
        insert_reversal(pool, original_id, action_id).await?;
    }

    let amount_base = fx_rate.map(|rate| trade_cash_flow_native(&trade) * rate);
    let trade_id = Uuid::new_v4().to_string();
    let created_at = now_iso();
    sqlx::query(
        r#"INSERT INTO portfolio_trade_events (
            id, event_kind, symbol, side, quantity, price, fees, currency, occurred_at,
            fx_rate, fx_source, amount_base, impacts_portfolio, reverses_trade_id,
            correction_of_trade_id, action_id, account, notes, created_at
        ) VALUES (?, 'trade', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?, ?, ?, ?)"#,
    )
    .bind(&trade_id)
    .bind(&trade.symbol)
    .bind(&trade.side)
    .bind(trade.quantity)
    .bind(trade.price)
    .bind(trade.fees)
    .bind(&trade.currency)
    .bind(&trade.occurred_at)
    .bind(fx_rate)
    .bind(&fx_source)
    .bind(amount_base)
    .bind(impacts_portfolio)
    .bind(&trade.corrects_trade_id)
    .bind(action_id)
    .bind(&trade.account)
    .bind(&trade.notes)
    .bind(&created_at)
    .execute(pool)
    .await?;

    if impacts_portfolio {
        insert_trade_cash_flow(pool, &trade_id, &trade, fx_rate.expect("resolved FX"), amount_base.expect("resolved amount")).await?;
        project_symbol_from_ledger(pool, &baseline).await?;
        recompute_weights_with_fx(pool, market_data.clone()).await?;
        record_portfolio_performance_snapshot(pool, market_data, "conversation_trade").await?;
    }

    Ok(TradeReceipt {
        trade_id,
        symbol: trade.symbol.clone(),
        impacts_portfolio,
        baseline_effective_at: baseline.effective_at,
        amount_base,
        fx_rate,
        fx_source,
        position: get_position(pool, &trade.symbol).await.ok(),
    })
}

pub async fn recent_trade_events(
    pool: &SqlitePool,
    symbol: &str,
    limit: i64,
) -> AppResult<Vec<PortfolioTradeEvent>> {
    let rows = sqlx::query(
        r#"SELECT id, event_kind, symbol, side, quantity, price, fees, currency,
                  occurred_at, fx_rate, fx_source, amount_base, impacts_portfolio,
                  reverses_trade_id, correction_of_trade_id, account, notes, created_at
        FROM portfolio_trade_events WHERE symbol = ?
        ORDER BY occurred_at DESC, created_at DESC LIMIT ?"#,
    )
    .bind(symbol.trim().to_ascii_uppercase())
    .bind(limit.clamp(1, 100))
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(trade_event_from_row).collect()
}

pub async fn record_current_position_baselines(pool: &SqlitePool, source: &str) -> AppResult<()> {
    let positions = list_positions(pool).await?;
    let effective_at = now_iso();
    for position in positions {
        record_position_baseline(pool, &position, source, &effective_at).await?;
    }
    Ok(())
}

async fn record_position_baseline(
    pool: &SqlitePool,
    position: &PortfolioPosition,
    source: &str,
    effective_at: &str,
) -> AppResult<()> {
    sqlx::query(
        r#"INSERT INTO portfolio_position_baselines (
            id, symbol, effective_at, name, asset_type, quantity, average_cost, currency,
            account, market, sector, notes, last_price, source, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&position.symbol)
    .bind(effective_at)
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
    .bind(source)
    .bind(now_iso())
    .execute(pool)
    .await?;
    Ok(())
}

async fn record_zero_position_baseline(
    pool: &SqlitePool,
    position: &PortfolioPosition,
    source: &str,
) -> AppResult<()> {
    let mut position = position.clone();
    position.quantity = 0.0;
    position.average_cost = 0.0;
    position.market_value = 0.0;
    record_position_baseline(pool, &position, source, &now_iso()).await
}

async fn ensure_position_baseline(pool: &SqlitePool, trade: &TradeRecord) -> AppResult<PositionBaseline> {
    if let Some(baseline) = latest_baseline(pool, &trade.symbol).await? {
        return Ok(baseline);
    }
    if let Ok(position) = get_position(pool, &trade.symbol).await {
        let effective_at = position.updated_at.clone();
        record_position_baseline(pool, &position, "conversation_migration", &effective_at).await?;
        return latest_baseline(pool, &trade.symbol)
            .await?
            .ok_or_else(|| AppError::internal("position baseline was not created"));
    }

    let baseline = PositionBaseline {
        symbol: trade.symbol.clone(),
        effective_at: "1970-01-01T00:00:00Z".to_string(),
        name: trade.symbol.clone(),
        asset_type: "stock".to_string(),
        quantity: 0.0,
        average_cost: 0.0,
        currency: trade.currency.clone(),
        account: trade.account.clone(),
        market: infer_market(&trade.symbol),
        sector: None,
        notes: None,
        last_price: Some(trade.price),
    };
    insert_baseline(pool, &baseline, "conversation_opening").await?;
    Ok(baseline)
}

async fn insert_baseline(pool: &SqlitePool, baseline: &PositionBaseline, source: &str) -> AppResult<()> {
    sqlx::query(
        r#"INSERT INTO portfolio_position_baselines (
            id, symbol, effective_at, name, asset_type, quantity, average_cost, currency,
            account, market, sector, notes, last_price, source, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&baseline.symbol)
    .bind(&baseline.effective_at)
    .bind(&baseline.name)
    .bind(&baseline.asset_type)
    .bind(baseline.quantity)
    .bind(baseline.average_cost)
    .bind(&baseline.currency)
    .bind(&baseline.account)
    .bind(&baseline.market)
    .bind(&baseline.sector)
    .bind(&baseline.notes)
    .bind(baseline.last_price)
    .bind(source)
    .bind(now_iso())
    .execute(pool)
    .await?;
    Ok(())
}

async fn latest_baseline(pool: &SqlitePool, symbol: &str) -> AppResult<Option<PositionBaseline>> {
    let row = sqlx::query(
        r#"SELECT symbol, effective_at, name, asset_type, quantity, average_cost, currency,
                  account, market, sector, notes, last_price
        FROM portfolio_position_baselines WHERE symbol = ?
        ORDER BY effective_at DESC, created_at DESC LIMIT 1"#,
    )
    .bind(symbol.trim().to_ascii_uppercase())
    .fetch_optional(pool)
    .await?;
    row.map(baseline_from_row).transpose()
}

async fn projected_position(
    pool: &SqlitePool,
    baseline: &PositionBaseline,
    additionally_excluded: Option<&str>,
) -> AppResult<(f64, f64)> {
    let rows = sqlx::query(
        r#"SELECT id, side, quantity, price, fees
        FROM portfolio_trade_events trade
        WHERE trade.symbol = ? AND trade.event_kind = 'trade' AND trade.impacts_portfolio = 1
          AND trade.occurred_at > ?
          AND NOT EXISTS (
              SELECT 1 FROM portfolio_trade_events reversal
              WHERE reversal.event_kind = 'reversal' AND reversal.reverses_trade_id = trade.id
          )
        ORDER BY trade.occurred_at ASC, trade.created_at ASC"#,
    )
    .bind(&baseline.symbol)
    .bind(&baseline.effective_at)
    .fetch_all(pool)
    .await?;
    let mut quantity = baseline.quantity;
    let mut average_cost = baseline.average_cost;
    for row in rows {
        let id: String = row.try_get("id")?;
        if additionally_excluded.is_some_and(|excluded| excluded == id) {
            continue;
        }
        let side: String = row.try_get("side")?;
        let trade_quantity: f64 = row.try_get("quantity")?;
        let price: f64 = row.try_get("price")?;
        let fees: f64 = row.try_get("fees")?;
        if side == "buy" {
            let next_quantity = quantity + trade_quantity;
            average_cost = if next_quantity <= 0.0 {
                0.0
            } else {
                (quantity * average_cost + trade_quantity * price + fees) / next_quantity
            };
            quantity = next_quantity;
        } else {
            if trade_quantity > quantity + 0.0000001 {
                return Err(AppError::bad_request("trade ledger contains an oversell"));
            }
            quantity -= trade_quantity;
            if quantity.abs() < 0.0000001 {
                quantity = 0.0;
                average_cost = 0.0;
            }
        }
    }
    Ok((quantity, average_cost))
}

async fn project_symbol_from_ledger(pool: &SqlitePool, baseline: &PositionBaseline) -> AppResult<()> {
    let (quantity, average_cost) = projected_position(pool, baseline, None).await?;
    if quantity <= 0.0 {
        sqlx::query("DELETE FROM portfolio_positions WHERE symbol = ?")
            .bind(&baseline.symbol)
            .execute(pool)
            .await?;
        return Ok(());
    }
    let last_price = get_position(pool, &baseline.symbol)
        .await
        .ok()
        .and_then(|position| position.last_price)
        .or(baseline.last_price)
        .unwrap_or(average_cost);
    let position = PortfolioPosition {
        symbol: baseline.symbol.clone(),
        name: baseline.name.clone(),
        asset_type: baseline.asset_type.clone(),
        quantity,
        average_cost,
        currency: baseline.currency.clone(),
        account: baseline.account.clone(),
        market: baseline.market.clone(),
        sector: baseline.sector.clone(),
        notes: baseline.notes.clone(),
        last_price: Some(last_price),
        market_value: last_price * quantity,
        market_value_base: last_price * quantity,
        unrealized_pnl: (last_price - average_cost) * quantity,
        unrealized_pnl_pct: ratio_option(last_price - average_cost, average_cost),
        period_profit_loss_base: None,
        period_return_pct: None,
        weight: 0.0,
        price_updated_at: None,
        price_stale: true,
        updated_at: now_iso(),
    };
    upsert_position(pool, &position).await
}

async fn insert_reversal(pool: &SqlitePool, original_id: &str, action_id: Option<&str>) -> AppResult<()> {
    let original = sqlx::query(
        r#"SELECT symbol, side, quantity, price, fees, currency, occurred_at,
                  amount_base, impacts_portfolio, fx_rate, fx_source
        FROM portfolio_trade_events WHERE id = ? AND event_kind = 'trade'"#,
    )
    .bind(original_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::not_found("trade to correct was not found"))?;
    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM portfolio_trade_events WHERE event_kind = 'reversal' AND reverses_trade_id = ?",
    )
    .bind(original_id)
    .fetch_one(pool)
    .await?;
    if exists > 0 {
        return Err(AppError::bad_request("trade was already reversed"));
    }
    let reversal_id = Uuid::new_v4().to_string();
    let amount_base = original.try_get::<Option<f64>, _>("amount_base")?.map(|value| -value);
    sqlx::query(
        r#"INSERT INTO portfolio_trade_events (
            id, event_kind, symbol, side, quantity, price, fees, currency, occurred_at,
            fx_rate, fx_source, amount_base, impacts_portfolio, reverses_trade_id,
            correction_of_trade_id, action_id, account, notes, created_at
        ) VALUES (?, 'reversal', ?, NULL, NULL, NULL, NULL, ?, ?, ?, ?, ?, ?, ?, NULL, ?, NULL, ?, ?)"#,
    )
    .bind(&reversal_id)
    .bind(original.try_get::<String, _>("symbol")?)
    .bind(original.try_get::<String, _>("currency")?)
    .bind(original.try_get::<String, _>("occurred_at")?)
    .bind(original.try_get::<Option<f64>, _>("fx_rate")?)
    .bind(original.try_get::<Option<String>, _>("fx_source")?)
    .bind(amount_base)
    .bind(original.try_get::<i64, _>("impacts_portfolio")? != 0)
    .bind(original_id)
    .bind(action_id)
    .bind(Some(format!("reversal of trade {original_id}")))
    .bind(now_iso())
    .execute(pool)
    .await?;
    if original.try_get::<i64, _>("impacts_portfolio")? != 0 {
        let original_side: String = original.try_get("side")?;
        let quantity: f64 = original.try_get("quantity")?;
        let price: f64 = original.try_get("price")?;
        let fees: f64 = original.try_get("fees")?;
        let native_amount = if original_side == "buy" {
            -(quantity * price + fees)
        } else {
            quantity * price - fees
        };
        insert_cash_flow_values(
            pool,
            LedgerCashFlow {
                trade_event_id: reversal_id,
                occurred_at: original.try_get("occurred_at")?,
                flow_type: "trade_reversal".to_string(),
                currency: original.try_get("currency")?,
                amount: native_amount,
                fx_rate: original.try_get::<Option<f64>, _>("fx_rate")?.unwrap_or(1.0),
                amount_base: amount_base.unwrap_or_default(),
                note: format!("reversal of trade {original_id}"),
            },
        )
        .await?;
    }
    Ok(())
}

async fn insert_trade_cash_flow(
    pool: &SqlitePool,
    trade_id: &str,
    trade: &TradeRecord,
    fx_rate: f64,
    amount_base: f64,
) -> AppResult<()> {
    insert_cash_flow_values(
        pool,
        LedgerCashFlow {
            trade_event_id: trade_id.to_string(),
            occurred_at: trade.occurred_at.clone(),
            flow_type: trade.side.clone(),
            currency: trade.currency.clone(),
            amount: trade_cash_flow_native(trade),
            fx_rate,
            amount_base,
            note: format!("conversation trade {} {}", trade.side, trade.symbol),
        },
    )
    .await
}

struct LedgerCashFlow {
    trade_event_id: String,
    occurred_at: String,
    flow_type: String,
    currency: String,
    amount: f64,
    fx_rate: f64,
    amount_base: f64,
    note: String,
}

async fn insert_cash_flow_values(pool: &SqlitePool, values: LedgerCashFlow) -> AppResult<()> {
    let cash_flow_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"INSERT INTO portfolio_cash_flows (
            id, occurred_at, flow_type, currency, amount, fx_rate, amount_base,
            note, source, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'conversation_trade', ?)"#,
    )
    .bind(&cash_flow_id)
    .bind(values.occurred_at)
    .bind(values.flow_type)
    .bind(values.currency)
    .bind(values.amount)
    .bind(values.fx_rate)
    .bind(values.amount_base)
    .bind(values.note)
    .bind(now_iso())
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO portfolio_trade_cash_flows (trade_event_id, cash_flow_id) VALUES (?, ?)",
    )
    .bind(values.trade_event_id)
    .bind(cash_flow_id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn historical_fx_rate(
    pool: &SqlitePool,
    market_data: Arc<dyn MarketDataProvider>,
    from_currency: &str,
    to_currency: &str,
    rate_date: &str,
) -> AppResult<ExchangeRate> {
    if let Some(row) = sqlx::query(
        r#"SELECT rate, source, updated_at FROM portfolio_historical_fx_rates
        WHERE from_currency = ? AND to_currency = ? AND rate_date = ?"#,
    )
    .bind(from_currency)
    .bind(to_currency)
    .bind(rate_date)
    .fetch_optional(pool)
    .await?
    {
        return Ok(ExchangeRate {
            from_currency: from_currency.to_string(),
            to_currency: to_currency.to_string(),
            rate: row.try_get("rate")?,
            source: row.try_get("source")?,
            updated_at: row.try_get("updated_at")?,
        });
    }
    let rate = market_data
        .exchange_rate_at(from_currency, to_currency, rate_date)
        .await
        .map_err(|error| AppError::bad_request(format!("historical FX unavailable: {error}")))?;
    sqlx::query(
        r#"INSERT OR REPLACE INTO portfolio_historical_fx_rates (
            from_currency, to_currency, rate_date, rate, source, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&rate.from_currency)
    .bind(&rate.to_currency)
    .bind(rate_date)
    .bind(rate.rate)
    .bind(&rate.source)
    .bind(&rate.updated_at)
    .execute(pool)
    .await?;
    Ok(rate)
}

fn normalize_trade(trade: &mut TradeRecord) -> AppResult<()> {
    trade.side = trade.side.trim().to_ascii_lowercase();
    if !matches!(trade.side.as_str(), "buy" | "sell") {
        return Err(AppError::bad_request("trade side must be buy or sell"));
    }
    trade.symbol = trade.symbol.trim().to_ascii_uppercase();
    trade.currency = trade.currency.trim().to_ascii_uppercase();
    if trade.symbol.is_empty() || trade.currency.is_empty() {
        return Err(AppError::bad_request("trade symbol and currency are required"));
    }
    if trade.quantity <= 0.0 || trade.price <= 0.0 || trade.fees < 0.0 {
        return Err(AppError::bad_request("trade quantity and price must be positive and fees non-negative"));
    }
    trade_date(&trade.occurred_at)?;
    Ok(())
}

fn trade_cash_flow_native(trade: &TradeRecord) -> f64 {
    if trade.side == "buy" {
        trade.quantity * trade.price + trade.fees
    } else {
        -(trade.quantity * trade.price - trade.fees)
    }
}

fn trade_date(value: &str) -> AppResult<&str> {
    let date = value.get(0..10).ok_or_else(|| AppError::bad_request("trade occurred_at must include YYYY-MM-DD"))?;
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|_| AppError::bad_request("trade occurred_at must include YYYY-MM-DD"))?;
    Ok(date)
}

fn timestamp_after(value: &str, baseline: &str) -> AppResult<bool> {
    let value = parse_timestamp(value)?;
    let baseline = parse_timestamp(baseline)?;
    Ok(value > baseline)
}

fn parse_timestamp(value: &str) -> AppResult<chrono::DateTime<chrono::Utc>> {
    if let Ok(timestamp) = chrono::DateTime::parse_from_rfc3339(value) {
        return Ok(timestamp.with_timezone(&chrono::Utc));
    }
    let date = chrono::NaiveDate::parse_from_str(value.get(0..10).unwrap_or(value), "%Y-%m-%d")
        .map_err(|_| AppError::bad_request("timestamp must be RFC3339 or YYYY-MM-DD"))?;
    Ok(chrono::DateTime::from_naive_utc_and_offset(
        date.and_hms_opt(23, 59, 59).expect("valid day end"),
        chrono::Utc,
    ))
}

fn baseline_from_row(row: sqlx::sqlite::SqliteRow) -> AppResult<PositionBaseline> {
    Ok(PositionBaseline {
        symbol: row.try_get("symbol")?,
        effective_at: row.try_get("effective_at")?,
        name: row.try_get("name")?,
        asset_type: row.try_get("asset_type")?,
        quantity: row.try_get("quantity")?,
        average_cost: row.try_get("average_cost")?,
        currency: row.try_get("currency")?,
        account: row.try_get("account")?,
        market: row.try_get("market")?,
        sector: row.try_get("sector")?,
        notes: row.try_get("notes")?,
        last_price: row.try_get("last_price")?,
    })
}

fn trade_event_from_row(row: sqlx::sqlite::SqliteRow) -> AppResult<PortfolioTradeEvent> {
    Ok(PortfolioTradeEvent {
        id: row.try_get("id")?,
        event_kind: row.try_get("event_kind")?,
        symbol: row.try_get("symbol")?,
        side: row.try_get("side")?,
        quantity: row.try_get("quantity")?,
        price: row.try_get("price")?,
        fees: row.try_get("fees")?,
        currency: row.try_get("currency")?,
        occurred_at: row.try_get("occurred_at")?,
        fx_rate: row.try_get("fx_rate")?,
        fx_source: row.try_get("fx_source")?,
        amount_base: row.try_get("amount_base")?,
        impacts_portfolio: row.try_get::<i64, _>("impacts_portfolio")? != 0,
        reverses_trade_id: row.try_get("reverses_trade_id")?,
        correction_of_trade_id: row.try_get("correction_of_trade_id")?,
        account: row.try_get("account")?,
        notes: row.try_get("notes")?,
        created_at: row.try_get("created_at")?,
    })
}
