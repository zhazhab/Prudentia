struct RefreshContext {
    market_data: Arc<dyn MarketDataProvider>,
    total_market_value_base: f64,
    quotes: HashMap<String, Result<MarketQuote, String>>,
    fx_rates: HashMap<String, Result<ExchangeRate, String>>,
}

impl RefreshContext {
    fn new(market_data: Arc<dyn MarketDataProvider>, total_market_value_base: f64) -> Self {
        Self {
            market_data,
            total_market_value_base,
            quotes: HashMap::new(),
            fx_rates: HashMap::new(),
        }
    }

    async fn quote(&mut self, symbol: &str) -> AppResult<MarketQuote> {
        let key = symbol.trim().to_ascii_uppercase();
        if !self.quotes.contains_key(&key) {
            let result = self
                .market_data
                .quote(&key)
                .await
                .map_err(|error| error.to_string());
            self.quotes.insert(key.clone(), result);
        }
        self.quotes
            .get(&key)
            .expect("quote cache entry")
            .clone()
            .map_err(AppError::internal)
    }

    async fn exchange_rate(&mut self, from_currency: &str) -> AppResult<FxValue> {
        if from_currency.eq_ignore_ascii_case(BASE_CURRENCY) {
            return Ok(FxValue {
                rate: 1.0,
                source: Some("identity".to_string()),
                updated_at: Some(now_iso()),
            });
        }

        let from_currency = from_currency.trim().to_ascii_uppercase();
        let key = format!("{from_currency}->{BASE_CURRENCY}");
        if !self.fx_rates.contains_key(&key) {
            let result = self
                .market_data
                .exchange_rate(&from_currency, BASE_CURRENCY)
                .await
                .map_err(|error| error.to_string());
            self.fx_rates.insert(key.clone(), result);
        }

        let rate = self
            .fx_rates
            .get(&key)
            .expect("fx cache entry")
            .clone()
            .map_err(AppError::internal)?;
        Ok(FxValue {
            rate: rate.rate,
            source: Some(rate.source),
            updated_at: Some(rate.updated_at),
        })
    }
}

async fn calculate_snapshot(
    context: &mut RefreshContext,
    decision_id: &str,
    legs: &[DecisionDeltaLeg],
) -> AppResult<DecisionDeltaSnapshot> {
    if legs.len() < 2 {
        return Err(AppError::bad_request("decision is not quantifiable"));
    }

    let actual = legs
        .iter()
        .find(|leg| leg.leg_kind == "actual")
        .ok_or_else(|| AppError::bad_request("actual leg is missing"))?;
    let baseline = legs
        .iter()
        .find(|leg| leg.leg_kind == "baseline")
        .ok_or_else(|| AppError::bad_request("baseline leg is missing"))?;

    let actual_value = leg_current_value(actual, context).await?;
    let baseline_value = leg_current_value(baseline, context).await?;
    let delta_value = actual_value.value - baseline_value.value;
    let portfolio_impact_pct = (context.total_market_value_base > 0.0)
        .then_some(delta_value / context.total_market_value_base);
    let now = now_iso();

    Ok(DecisionDeltaSnapshot {
        id: Uuid::new_v4().to_string(),
        decision_id: decision_id.to_string(),
        as_of_date: now.clone(),
        actual_value: round_money(actual_value.value),
        baseline_value: round_money(baseline_value.value),
        delta_value: round_money(delta_value),
        delta_pct: (baseline_value.value.abs() > f64::EPSILON)
            .then_some(delta_value / baseline_value.value.abs()),
        portfolio_impact_pct,
        price_used: actual_value.price.or(baseline_value.price),
        price_source: actual_value.price_source.or(baseline_value.price_source),
        price_updated_at: actual_value
            .price_updated_at
            .or(baseline_value.price_updated_at),
        fx_rate_used: actual_value.fx_rate.or(baseline_value.fx_rate),
        fx_source: actual_value.fx_source.or(baseline_value.fx_source),
        fx_updated_at: actual_value.fx_updated_at.or(baseline_value.fx_updated_at),
        price_stale: false,
        fx_stale: false,
        created_at: now,
    })
}

#[derive(Debug)]
struct LegValue {
    value: f64,
    price: Option<f64>,
    price_source: Option<String>,
    price_updated_at: Option<String>,
    fx_rate: Option<f64>,
    fx_source: Option<String>,
    fx_updated_at: Option<String>,
}

async fn leg_current_value(
    leg: &DecisionDeltaLeg,
    context: &mut RefreshContext,
) -> AppResult<LegValue> {
    if let Some(symbol) = &leg.symbol {
        let quote = context.quote(symbol).await?;
        let currency = quote
            .currency
            .clone()
            .unwrap_or_else(|| leg.currency.clone());
        let fx = context.exchange_rate(&currency).await?;
        let quantity = leg.quantity.unwrap_or_default();
        return Ok(LegValue {
            value: quantity * quote.price * fx.rate,
            price: Some(quote.price),
            price_source: Some(quote.source),
            price_updated_at: Some(quote.updated_at),
            fx_rate: Some(fx.rate),
            fx_source: fx.source,
            fx_updated_at: fx.updated_at,
        });
    }

    let fx = context.exchange_rate(&leg.currency).await?;
    Ok(LegValue {
        value: leg.notional.unwrap_or_default() * fx.rate,
        price: None,
        price_source: None,
        price_updated_at: None,
        fx_rate: Some(fx.rate),
        fx_source: fx.source,
        fx_updated_at: fx.updated_at,
    })
}

struct FxValue {
    rate: f64,
    source: Option<String>,
    updated_at: Option<String>,
}

async fn insert_snapshot(pool: &SqlitePool, snapshot: &DecisionDeltaSnapshot) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO decision_delta_snapshots (
            id, decision_id, as_of_date, actual_value, baseline_value, delta_value,
            delta_pct, portfolio_impact_pct, price_used, price_source, price_updated_at,
            fx_rate_used, fx_source, fx_updated_at, price_stale, fx_stale, created_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&snapshot.id)
    .bind(&snapshot.decision_id)
    .bind(&snapshot.as_of_date)
    .bind(snapshot.actual_value)
    .bind(snapshot.baseline_value)
    .bind(snapshot.delta_value)
    .bind(snapshot.delta_pct)
    .bind(snapshot.portfolio_impact_pct)
    .bind(snapshot.price_used)
    .bind(&snapshot.price_source)
    .bind(&snapshot.price_updated_at)
    .bind(snapshot.fx_rate_used)
    .bind(&snapshot.fx_source)
    .bind(&snapshot.fx_updated_at)
    .bind(snapshot.price_stale)
    .bind(snapshot.fx_stale)
    .bind(&snapshot.created_at)
    .execute(pool)
    .await?;
    Ok(())
}

fn stale_snapshot_from_previous(previous: DecisionDeltaSnapshot) -> DecisionDeltaSnapshot {
    let now = now_iso();
    DecisionDeltaSnapshot {
        id: Uuid::new_v4().to_string(),
        as_of_date: now.clone(),
        price_stale: true,
        fx_stale: true,
        created_at: now,
        ..previous
    }
}
