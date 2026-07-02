use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    routing::{get, patch, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};
use uuid::Uuid;

use crate::{
    decision::{self, Decision},
    error::{AppError, AppResult},
    investment_system::{self, InvestmentSystem, UpdateInvestmentSystemRequest},
    locale::Locale,
    market_data::{ExchangeRate, MarketDataProvider, MarketQuote},
    portfolio,
    state::AppState,
    time::now_iso,
};

const BASE_CURRENCY: &str = "CNY";
const DEFAULT_SNAPSHOT_LIMIT: usize = 90;
const MAX_SNAPSHOT_LIMIT: usize = 365;

#[derive(Debug, Clone)]
pub struct DecisionDeltaInput {
    pub action: String,
    pub symbol: Option<String>,
    pub quantity: Option<f64>,
    pub notional: Option<f64>,
    pub price: Option<f64>,
    pub currency: Option<String>,
    pub baseline_type: Option<String>,
    pub hypothetical_notional: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionDeltaLeg {
    pub id: String,
    pub decision_id: String,
    pub leg_kind: String,
    pub baseline_type: Option<String>,
    pub symbol: Option<String>,
    pub quantity: Option<f64>,
    pub notional: Option<f64>,
    pub price: Option<f64>,
    pub currency: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionDeltaSnapshot {
    pub id: String,
    pub decision_id: String,
    pub as_of_date: String,
    pub actual_value: f64,
    pub baseline_value: f64,
    pub delta_value: f64,
    pub delta_pct: Option<f64>,
    pub portfolio_impact_pct: Option<f64>,
    pub price_used: Option<f64>,
    pub price_source: Option<String>,
    pub price_updated_at: Option<String>,
    pub fx_rate_used: Option<f64>,
    pub fx_source: Option<String>,
    pub fx_updated_at: Option<String>,
    pub price_stale: bool,
    pub fx_stale: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionDeltaReview {
    pub decision_id: String,
    pub notes: String,
    pub thesis_evidence: Vec<String>,
    pub disconfirming_evidence: Vec<String>,
    pub lessons: Vec<String>,
    pub candidate_principles: Vec<String>,
    pub candidate_checklist_items: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionDeltaDetail {
    pub decision: Decision,
    pub legs: Vec<DecisionDeltaLeg>,
    pub quantifiable: bool,
    pub latest_snapshot: Option<DecisionDeltaSnapshot>,
    pub snapshots: Vec<DecisionDeltaSnapshot>,
    pub review: Option<DecisionDeltaReview>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionDeltaTimeline {
    pub summary: DecisionDeltaTimelineSummary,
    pub items: Vec<DecisionDeltaTimelineItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionDeltaTimelineSummary {
    pub label: String,
    pub visible_decisions_count: usize,
    pub quantifiable_decisions_count: usize,
    pub positive_delta_count: usize,
    pub negative_delta_count: usize,
    pub sum_delta_value: f64,
    pub sum_portfolio_impact_pct: Option<f64>,
    pub last_refreshed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionDeltaTimelineItem {
    pub decision: Decision,
    pub quantifiable: bool,
    pub reviewed: bool,
    pub latest_snapshot: Option<DecisionDeltaSnapshot>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DecisionDeltaTimelineQuery {
    pub symbol: Option<String>,
    pub action: Option<String>,
    pub year: Option<String>,
    pub delta: Option<String>,
    pub stale: Option<String>,
    pub reviewed: Option<String>,
    pub sort: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct DecisionDeltaDetailQuery {
    snapshot_limit: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RefreshDecisionDeltasRequest {
    pub decision_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RefreshDecisionDeltasResult {
    pub refreshed: usize,
    pub failed: usize,
    pub failures: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DecisionDeltaReviewRequest {
    pub notes: String,
    pub thesis_evidence: Vec<String>,
    pub disconfirming_evidence: Vec<String>,
    pub lessons: Vec<String>,
    pub candidate_principles: Vec<String>,
    pub candidate_checklist_items: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdoptDecisionDeltaCandidatesRequest {
    pub principles: Vec<String>,
    pub checklist_items: Vec<String>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/timeline", get(timeline_handler))
        .route("/refresh", post(refresh_handler))
        .route("/{decision_id}", get(detail_handler))
        .route("/{decision_id}/review", patch(save_review_handler))
        .route("/{decision_id}/adopt", post(adopt_candidates_handler))
}

async fn timeline_handler(
    State(state): State<AppState>,
    Query(query): Query<DecisionDeltaTimelineQuery>,
) -> AppResult<Json<DecisionDeltaTimeline>> {
    Ok(Json(timeline(&state.pool, query).await?))
}

async fn refresh_handler(
    State(state): State<AppState>,
    Json(request): Json<RefreshDecisionDeltasRequest>,
) -> AppResult<Json<RefreshDecisionDeltasResult>> {
    Ok(Json(
        refresh(&state.pool, state.market_data.clone(), request).await?,
    ))
}

async fn detail_handler(
    State(state): State<AppState>,
    Path(decision_id): Path<String>,
    Query(query): Query<DecisionDeltaDetailQuery>,
) -> AppResult<Json<DecisionDeltaDetail>> {
    Ok(Json(
        get_detail_with_limit(
            &state.pool,
            &decision_id,
            snapshot_limit(query.snapshot_limit),
        )
        .await?,
    ))
}

async fn save_review_handler(
    State(state): State<AppState>,
    Path(decision_id): Path<String>,
    Json(request): Json<DecisionDeltaReviewRequest>,
) -> AppResult<Json<DecisionDeltaReview>> {
    Ok(Json(save_review(&state.pool, &decision_id, request).await?))
}

async fn adopt_candidates_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(decision_id): Path<String>,
    Json(request): Json<AdoptDecisionDeltaCandidatesRequest>,
) -> AppResult<Json<InvestmentSystem>> {
    Ok(Json(
        adopt_candidates(
            &state.pool,
            &decision_id,
            request,
            Locale::from_headers(&headers),
        )
        .await?,
    ))
}

pub async fn create_legs_for_decision(
    pool: &SqlitePool,
    decision_id: &str,
    input: DecisionDeltaInput,
) -> AppResult<Vec<DecisionDeltaLeg>> {
    let legs = legs_from_input(decision_id, input)?;
    for leg in &legs {
        insert_leg(pool, leg).await?;
    }
    Ok(legs)
}

pub async fn get_detail(pool: &SqlitePool, decision_id: &str) -> AppResult<DecisionDeltaDetail> {
    get_detail_with_limit(pool, decision_id, DEFAULT_SNAPSHOT_LIMIT).await
}

async fn get_detail_with_limit(
    pool: &SqlitePool,
    decision_id: &str,
    snapshot_limit: usize,
) -> AppResult<DecisionDeltaDetail> {
    let decision = decision::get(pool, decision_id).await?;
    let legs = list_legs(pool, decision_id).await?;
    let latest_snapshot = latest_snapshot(pool, decision_id).await?;
    let snapshots = list_snapshots(pool, decision_id, snapshot_limit).await?;
    let review = get_review_optional(pool, decision_id).await?;

    Ok(DecisionDeltaDetail {
        decision,
        quantifiable: legs.len() >= 2,
        legs,
        latest_snapshot,
        snapshots,
        review,
    })
}

pub async fn refresh(
    pool: &SqlitePool,
    market_data: Arc<dyn MarketDataProvider>,
    request: RefreshDecisionDeltasRequest,
) -> AppResult<RefreshDecisionDeltasResult> {
    let decision_ids = match request.decision_ids {
        Some(ids) => ids,
        None => list_quantifiable_decision_ids(pool).await?,
    };

    let mut result = RefreshDecisionDeltasResult {
        refreshed: 0,
        failed: 0,
        failures: Vec::new(),
    };
    if decision_ids.is_empty() {
        return Ok(result);
    }

    let portfolio_summary = portfolio::summary(pool).await?;
    let mut context = RefreshContext::new(market_data, portfolio_summary.total_market_value_base);
    let legs_by_decision = list_legs_for_decisions(pool, &decision_ids).await?;

    for decision_id in decision_ids {
        let legs = legs_by_decision
            .get(&decision_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        match calculate_snapshot(&mut context, &decision_id, legs).await {
            Ok(snapshot) => {
                insert_snapshot(pool, &snapshot).await?;
                result.refreshed += 1;
            }
            Err(error) => {
                result.failed += 1;
                result.failures.push(format!("{decision_id}: {error}"));
                if let Some(previous) = latest_snapshot(pool, &decision_id).await? {
                    let stale = stale_snapshot_from_previous(previous);
                    insert_snapshot(pool, &stale).await?;
                }
            }
        }
    }

    Ok(result)
}

pub async fn timeline(
    pool: &SqlitePool,
    query: DecisionDeltaTimelineQuery,
) -> AppResult<DecisionDeltaTimeline> {
    let mut items = timeline_items(pool).await?;
    apply_filters(&mut items, &query);
    apply_sort(&mut items, query.sort.as_deref());

    let mut summary = DecisionDeltaTimelineSummary {
        label: "sum_of_decision_deltas".to_string(),
        visible_decisions_count: items.len(),
        quantifiable_decisions_count: items.iter().filter(|item| item.quantifiable).count(),
        positive_delta_count: 0,
        negative_delta_count: 0,
        sum_delta_value: 0.0,
        sum_portfolio_impact_pct: None,
        last_refreshed_at: None,
    };

    let mut impact_sum = 0.0;
    let mut has_impact = false;
    for snapshot in items
        .iter()
        .filter_map(|item| item.latest_snapshot.as_ref())
    {
        summary.sum_delta_value += snapshot.delta_value;
        if snapshot.delta_value > 0.0 {
            summary.positive_delta_count += 1;
        }
        if snapshot.delta_value < 0.0 {
            summary.negative_delta_count += 1;
        }
        if let Some(impact) = snapshot.portfolio_impact_pct {
            impact_sum += impact;
            has_impact = true;
        }
        if summary
            .last_refreshed_at
            .as_deref()
            .is_none_or(|current| snapshot.created_at.as_str() > current)
        {
            summary.last_refreshed_at = Some(snapshot.created_at.clone());
        }
    }
    if has_impact {
        summary.sum_portfolio_impact_pct = Some(impact_sum);
    }

    Ok(DecisionDeltaTimeline { summary, items })
}

pub async fn save_review(
    pool: &SqlitePool,
    decision_id: &str,
    request: DecisionDeltaReviewRequest,
) -> AppResult<DecisionDeltaReview> {
    decision::get(pool, decision_id).await?;
    let now = now_iso();
    let existing = get_review_optional(pool, decision_id).await?;
    let review = DecisionDeltaReview {
        decision_id: decision_id.to_string(),
        notes: request.notes.trim().to_string(),
        thesis_evidence: clean_list(request.thesis_evidence),
        disconfirming_evidence: clean_list(request.disconfirming_evidence),
        lessons: clean_list(request.lessons),
        candidate_principles: clean_list(request.candidate_principles),
        candidate_checklist_items: clean_list(request.candidate_checklist_items),
        created_at: existing
            .as_ref()
            .map(|value| value.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now,
    };

    sqlx::query(
        r#"
        INSERT INTO decision_delta_reviews (
            decision_id, notes, thesis_evidence_json, disconfirming_evidence_json,
            lessons_json, candidate_principles_json, candidate_checklist_items_json,
            created_at, updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(decision_id) DO UPDATE SET
            notes = excluded.notes,
            thesis_evidence_json = excluded.thesis_evidence_json,
            disconfirming_evidence_json = excluded.disconfirming_evidence_json,
            lessons_json = excluded.lessons_json,
            candidate_principles_json = excluded.candidate_principles_json,
            candidate_checklist_items_json = excluded.candidate_checklist_items_json,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(&review.decision_id)
    .bind(&review.notes)
    .bind(serde_json::to_string(&review.thesis_evidence)?)
    .bind(serde_json::to_string(&review.disconfirming_evidence)?)
    .bind(serde_json::to_string(&review.lessons)?)
    .bind(serde_json::to_string(&review.candidate_principles)?)
    .bind(serde_json::to_string(&review.candidate_checklist_items)?)
    .bind(&review.created_at)
    .bind(&review.updated_at)
    .execute(pool)
    .await?;

    Ok(review)
}

pub async fn adopt_candidates(
    pool: &SqlitePool,
    decision_id: &str,
    request: AdoptDecisionDeltaCandidatesRequest,
    locale: Locale,
) -> AppResult<InvestmentSystem> {
    let review = get_review_optional(pool, decision_id)
        .await?
        .ok_or_else(|| AppError::not_found("decision delta review not found"))?;
    let selected_principles = matching_candidates(request.principles, &review.candidate_principles);
    let selected_checklist =
        matching_candidates(request.checklist_items, &review.candidate_checklist_items);
    if selected_principles.is_empty() && selected_checklist.is_empty() {
        return Err(AppError::bad_request(
            "no selected candidates match this review",
        ));
    }

    let mut system = investment_system::get_or_default_with_locale(pool, locale).await?;
    system.principles.extend(selected_principles);
    system.checklist_items.extend(selected_checklist);

    investment_system::update_with_locale(
        pool,
        UpdateInvestmentSystemRequest {
            principles: Some(dedupe(system.principles)),
            checklist_items: Some(dedupe(system.checklist_items)),
            circle_of_competence: Some(system.circle_of_competence),
            decision_rules: Some(system.decision_rules),
        },
        locale,
    )
    .await
}

fn legs_from_input(
    decision_id: &str,
    input: DecisionDeltaInput,
) -> AppResult<Vec<DecisionDeltaLeg>> {
    if !has_quantification(&input) {
        return Ok(Vec::new());
    }

    let action = input.action.trim().to_ascii_lowercase();
    let symbol = input
        .symbol
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_uppercase);
    let currency = input
        .currency
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_uppercase)
        .unwrap_or_else(|| BASE_CURRENCY.to_string());
    let now = now_iso();

    match action.as_str() {
        "buy" | "add" => {
            let symbol = symbol.ok_or_else(|| AppError::bad_request("symbol is required"))?;
            let price = positive(input.price, "price")?;
            let quantity = quantity_from_input(input.quantity, input.notional, price)?;
            let notional = input.notional.unwrap_or(quantity * price);
            Ok(vec![
                leg(
                    decision_id,
                    "actual",
                    None,
                    Some(symbol),
                    Some(quantity),
                    None,
                    Some(price),
                    &currency,
                    &now,
                ),
                leg(
                    decision_id,
                    "baseline",
                    Some(input.baseline_type.unwrap_or_else(|| "cash".to_string())),
                    None,
                    None,
                    Some(notional),
                    None,
                    &currency,
                    &now,
                ),
            ])
        }
        "sell" | "trim" => {
            let symbol = symbol.ok_or_else(|| AppError::bad_request("symbol is required"))?;
            let price = positive(input.price, "price")?;
            let quantity = positive(input.quantity, "quantity")?;
            let notional = input.notional.unwrap_or(quantity * price);
            Ok(vec![
                leg(
                    decision_id,
                    "actual",
                    Some("cash".to_string()),
                    None,
                    None,
                    Some(notional),
                    None,
                    &currency,
                    &now,
                ),
                leg(
                    decision_id,
                    "baseline",
                    Some(
                        input
                            .baseline_type
                            .unwrap_or_else(|| "continue_holding".to_string()),
                    ),
                    Some(symbol),
                    Some(quantity),
                    None,
                    Some(price),
                    &currency,
                    &now,
                ),
            ])
        }
        "watch" | "skip" => {
            let Some(hypothetical_notional) = input.hypothetical_notional else {
                return Ok(Vec::new());
            };
            let symbol = symbol.ok_or_else(|| AppError::bad_request("symbol is required"))?;
            let price = positive(input.price, "price")?;
            let notional = positive(Some(hypothetical_notional), "hypothetical_notional")?;
            let quantity = notional / price;
            Ok(vec![
                leg(
                    decision_id,
                    "actual",
                    Some("cash".to_string()),
                    None,
                    None,
                    Some(notional),
                    None,
                    &currency,
                    &now,
                ),
                leg(
                    decision_id,
                    "baseline",
                    Some(
                        input
                            .baseline_type
                            .unwrap_or_else(|| "hypothetical_buy".to_string()),
                    ),
                    Some(symbol),
                    Some(quantity),
                    None,
                    Some(price),
                    &currency,
                    &now,
                ),
            ])
        }
        _ => Ok(Vec::new()),
    }
}

fn has_quantification(input: &DecisionDeltaInput) -> bool {
    input.quantity.is_some()
        || input.notional.is_some()
        || input.price.is_some()
        || input.currency.is_some()
        || input.hypothetical_notional.is_some()
}

fn leg(
    decision_id: &str,
    leg_kind: &str,
    baseline_type: Option<String>,
    symbol: Option<String>,
    quantity: Option<f64>,
    notional: Option<f64>,
    price: Option<f64>,
    currency: &str,
    now: &str,
) -> DecisionDeltaLeg {
    DecisionDeltaLeg {
        id: Uuid::new_v4().to_string(),
        decision_id: decision_id.to_string(),
        leg_kind: leg_kind.to_string(),
        baseline_type,
        symbol,
        quantity,
        notional,
        price,
        currency: currency.to_ascii_uppercase(),
        created_at: now.to_string(),
        updated_at: now.to_string(),
    }
}

async fn insert_leg(pool: &SqlitePool, leg: &DecisionDeltaLeg) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO decision_delta_legs (
            id, decision_id, leg_kind, baseline_type, symbol, quantity,
            notional, price, currency, created_at, updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&leg.id)
    .bind(&leg.decision_id)
    .bind(&leg.leg_kind)
    .bind(&leg.baseline_type)
    .bind(&leg.symbol)
    .bind(leg.quantity)
    .bind(leg.notional)
    .bind(leg.price)
    .bind(&leg.currency)
    .bind(&leg.created_at)
    .bind(&leg.updated_at)
    .execute(pool)
    .await?;
    Ok(())
}

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

async fn list_legs(pool: &SqlitePool, decision_id: &str) -> AppResult<Vec<DecisionDeltaLeg>> {
    let rows = sqlx::query(
        r#"
        SELECT id, decision_id, leg_kind, baseline_type, symbol, quantity,
               notional, price, currency, created_at, updated_at
        FROM decision_delta_legs
        WHERE decision_id = ?
        ORDER BY CASE leg_kind WHEN 'actual' THEN 0 ELSE 1 END
        "#,
    )
    .bind(decision_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter().map(leg_from_row).collect()
}

async fn list_legs_for_decisions(
    pool: &SqlitePool,
    decision_ids: &[String],
) -> AppResult<HashMap<String, Vec<DecisionDeltaLeg>>> {
    if decision_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let mut query = QueryBuilder::<Sqlite>::new(
        r#"
        SELECT id, decision_id, leg_kind, baseline_type, symbol, quantity,
               notional, price, currency, created_at, updated_at
        FROM decision_delta_legs
        WHERE decision_id IN (
        "#,
    );
    let mut separated = query.separated(", ");
    for decision_id in decision_ids {
        separated.push_bind(decision_id);
    }
    separated.push_unseparated(
        r#")
        ORDER BY decision_id, CASE leg_kind WHEN 'actual' THEN 0 ELSE 1 END
        "#,
    );

    let rows = query.build().fetch_all(pool).await?;
    let mut by_decision: HashMap<String, Vec<DecisionDeltaLeg>> = HashMap::new();
    for row in rows {
        let leg = leg_from_row(row)?;
        by_decision
            .entry(leg.decision_id.clone())
            .or_default()
            .push(leg);
    }
    Ok(by_decision)
}

async fn list_snapshots(
    pool: &SqlitePool,
    decision_id: &str,
    limit: usize,
) -> AppResult<Vec<DecisionDeltaSnapshot>> {
    let rows = sqlx::query(
        r#"
        SELECT id, decision_id, as_of_date, actual_value, baseline_value, delta_value,
               delta_pct, portfolio_impact_pct, price_used, price_source, price_updated_at,
               fx_rate_used, fx_source, fx_updated_at, price_stale, fx_stale, created_at
        FROM decision_delta_snapshots
        WHERE decision_id = ?
        ORDER BY created_at DESC, id DESC
        LIMIT ?
        "#,
    )
    .bind(decision_id)
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;

    rows.into_iter().map(snapshot_from_row).collect()
}

async fn latest_snapshot(
    pool: &SqlitePool,
    decision_id: &str,
) -> AppResult<Option<DecisionDeltaSnapshot>> {
    let row = sqlx::query(
        r#"
        SELECT id, decision_id, as_of_date, actual_value, baseline_value, delta_value,
               delta_pct, portfolio_impact_pct, price_used, price_source, price_updated_at,
               fx_rate_used, fx_source, fx_updated_at, price_stale, fx_stale, created_at
        FROM decision_delta_snapshots
        WHERE decision_id = ?
        ORDER BY created_at DESC, id DESC
        LIMIT 1
        "#,
    )
    .bind(decision_id)
    .fetch_optional(pool)
    .await?;

    row.map(snapshot_from_row).transpose()
}

async fn list_quantifiable_decision_ids(pool: &SqlitePool) -> AppResult<Vec<String>> {
    let rows = sqlx::query(
        r#"
        SELECT decision_id
        FROM decision_delta_legs
        GROUP BY decision_id
        HAVING COUNT(*) >= 2
        "#,
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| row.try_get("decision_id").map_err(AppError::from))
        .collect()
}

async fn timeline_items(pool: &SqlitePool) -> AppResult<Vec<DecisionDeltaTimelineItem>> {
    let rows = sqlx::query(
        r#"
        SELECT
            d.id, d.memo_id, d.symbol, d.action, d.rationale, d.confidence,
            d.expected_outcome, d.review_date, d.created_at,
            COALESCE(leg_counts.leg_count, 0) AS leg_count,
            reviews.decision_id IS NOT NULL AS reviewed,
            snapshots.id AS snapshot_id,
            snapshots.decision_id AS snapshot_decision_id,
            snapshots.as_of_date AS snapshot_as_of_date,
            snapshots.actual_value AS snapshot_actual_value,
            snapshots.baseline_value AS snapshot_baseline_value,
            snapshots.delta_value AS snapshot_delta_value,
            snapshots.delta_pct AS snapshot_delta_pct,
            snapshots.portfolio_impact_pct AS snapshot_portfolio_impact_pct,
            snapshots.price_used AS snapshot_price_used,
            snapshots.price_source AS snapshot_price_source,
            snapshots.price_updated_at AS snapshot_price_updated_at,
            snapshots.fx_rate_used AS snapshot_fx_rate_used,
            snapshots.fx_source AS snapshot_fx_source,
            snapshots.fx_updated_at AS snapshot_fx_updated_at,
            snapshots.price_stale AS snapshot_price_stale,
            snapshots.fx_stale AS snapshot_fx_stale,
            snapshots.created_at AS snapshot_created_at
        FROM decisions d
        LEFT JOIN (
            SELECT decision_id, COUNT(*) AS leg_count
            FROM decision_delta_legs
            GROUP BY decision_id
        ) leg_counts ON leg_counts.decision_id = d.id
        LEFT JOIN decision_delta_reviews reviews ON reviews.decision_id = d.id
        LEFT JOIN decision_delta_snapshots snapshots ON snapshots.id = (
            SELECT latest.id
            FROM decision_delta_snapshots latest
            WHERE latest.decision_id = d.id
            ORDER BY latest.created_at DESC, latest.id DESC
            LIMIT 1
        )
        ORDER BY d.created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|row| {
            Ok(DecisionDeltaTimelineItem {
                decision: decision_from_timeline_row(&row)?,
                quantifiable: row.try_get::<i64, _>("leg_count")? >= 2,
                reviewed: row.try_get::<bool, _>("reviewed")?,
                latest_snapshot: snapshot_from_timeline_row(&row)?,
            })
        })
        .collect()
}

fn apply_filters(items: &mut Vec<DecisionDeltaTimelineItem>, query: &DecisionDeltaTimelineQuery) {
    if let Some(symbol) = clean_option(query.symbol.clone()).map(|value| value.to_ascii_uppercase())
    {
        items.retain(|item| {
            item.decision
                .symbol
                .as_deref()
                .is_some_and(|item_symbol| item_symbol.eq_ignore_ascii_case(&symbol))
        });
    }
    if let Some(action) = clean_option(query.action.clone()).map(|value| value.to_ascii_lowercase())
    {
        items.retain(|item| item.decision.action.eq_ignore_ascii_case(&action));
    }
    if let Some(year) = clean_option(query.year.clone()) {
        items.retain(|item| item.decision.created_at.starts_with(&year));
    }
    if let Some(delta) = clean_option(query.delta.clone()).map(|value| value.to_ascii_lowercase()) {
        items.retain(
            |item| match (delta.as_str(), item.latest_snapshot.as_ref()) {
                ("positive", Some(snapshot)) => snapshot.delta_value > 0.0,
                ("negative", Some(snapshot)) => snapshot.delta_value < 0.0,
                ("zero", Some(snapshot)) => snapshot.delta_value == 0.0,
                ("none", None) => true,
                _ => false,
            },
        );
    }
    if let Some(stale) = parse_bool(query.stale.as_deref()) {
        items.retain(|item| {
            item.latest_snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.price_stale || snapshot.fx_stale)
                == stale
        });
    }
    if let Some(reviewed) = parse_bool(query.reviewed.as_deref()) {
        items.retain(|item| item.reviewed == reviewed);
    }
}

fn apply_sort(items: &mut [DecisionDeltaTimelineItem], sort: Option<&str>) {
    match sort.unwrap_or("date") {
        "absolute_delta" => items.sort_by(|left, right| {
            snapshot_abs_delta(right)
                .partial_cmp(&snapshot_abs_delta(left))
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        "portfolio_impact" => items.sort_by(|left, right| {
            snapshot_impact(right)
                .partial_cmp(&snapshot_impact(left))
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        "stale" => items.sort_by_key(|item| {
            !item
                .latest_snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.price_stale || snapshot.fx_stale)
        }),
        _ => items.sort_by(|left, right| right.decision.created_at.cmp(&left.decision.created_at)),
    }
}

fn snapshot_abs_delta(item: &DecisionDeltaTimelineItem) -> f64 {
    item.latest_snapshot
        .as_ref()
        .map(|snapshot| snapshot.delta_value.abs())
        .unwrap_or(0.0)
}

fn snapshot_impact(item: &DecisionDeltaTimelineItem) -> f64 {
    item.latest_snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.portfolio_impact_pct)
        .map(f64::abs)
        .unwrap_or(0.0)
}

async fn get_review_optional(
    pool: &SqlitePool,
    decision_id: &str,
) -> AppResult<Option<DecisionDeltaReview>> {
    let row = sqlx::query(
        r#"
        SELECT decision_id, notes, thesis_evidence_json, disconfirming_evidence_json,
               lessons_json, candidate_principles_json, candidate_checklist_items_json,
               created_at, updated_at
        FROM decision_delta_reviews
        WHERE decision_id = ?
        "#,
    )
    .bind(decision_id)
    .fetch_optional(pool)
    .await?;

    row.map(review_from_row).transpose()
}

fn leg_from_row(row: sqlx::sqlite::SqliteRow) -> AppResult<DecisionDeltaLeg> {
    Ok(DecisionDeltaLeg {
        id: row.try_get("id")?,
        decision_id: row.try_get("decision_id")?,
        leg_kind: row.try_get("leg_kind")?,
        baseline_type: row.try_get("baseline_type")?,
        symbol: row.try_get("symbol")?,
        quantity: row.try_get("quantity")?,
        notional: row.try_get("notional")?,
        price: row.try_get("price")?,
        currency: row.try_get("currency")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn decision_from_timeline_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<Decision> {
    Ok(Decision {
        id: row.try_get("id")?,
        memo_id: row.try_get("memo_id")?,
        symbol: row.try_get("symbol")?,
        action: row.try_get("action")?,
        rationale: row.try_get("rationale")?,
        confidence: row.try_get("confidence")?,
        expected_outcome: row.try_get("expected_outcome")?,
        review_date: row.try_get("review_date")?,
        created_at: row.try_get("created_at")?,
    })
}

fn snapshot_from_timeline_row(
    row: &sqlx::sqlite::SqliteRow,
) -> AppResult<Option<DecisionDeltaSnapshot>> {
    let Some(id) = row.try_get::<Option<String>, _>("snapshot_id")? else {
        return Ok(None);
    };

    Ok(Some(DecisionDeltaSnapshot {
        id,
        decision_id: required_alias(row, "snapshot_decision_id")?,
        as_of_date: required_alias(row, "snapshot_as_of_date")?,
        actual_value: required_alias(row, "snapshot_actual_value")?,
        baseline_value: required_alias(row, "snapshot_baseline_value")?,
        delta_value: required_alias(row, "snapshot_delta_value")?,
        delta_pct: row.try_get("snapshot_delta_pct")?,
        portfolio_impact_pct: row.try_get("snapshot_portfolio_impact_pct")?,
        price_used: row.try_get("snapshot_price_used")?,
        price_source: row.try_get("snapshot_price_source")?,
        price_updated_at: row.try_get("snapshot_price_updated_at")?,
        fx_rate_used: row.try_get("snapshot_fx_rate_used")?,
        fx_source: row.try_get("snapshot_fx_source")?,
        fx_updated_at: row.try_get("snapshot_fx_updated_at")?,
        price_stale: required_alias::<i64>(row, "snapshot_price_stale")? != 0,
        fx_stale: required_alias::<i64>(row, "snapshot_fx_stale")? != 0,
        created_at: required_alias(row, "snapshot_created_at")?,
    }))
}

fn required_alias<T>(row: &sqlx::sqlite::SqliteRow, name: &str) -> AppResult<T>
where
    for<'a> T: sqlx::Decode<'a, Sqlite> + sqlx::Type<Sqlite>,
{
    row.try_get::<Option<T>, _>(name)?
        .ok_or_else(|| AppError::internal(format!("{name} is missing")))
}

fn snapshot_from_row(row: sqlx::sqlite::SqliteRow) -> AppResult<DecisionDeltaSnapshot> {
    Ok(DecisionDeltaSnapshot {
        id: row.try_get("id")?,
        decision_id: row.try_get("decision_id")?,
        as_of_date: row.try_get("as_of_date")?,
        actual_value: row.try_get("actual_value")?,
        baseline_value: row.try_get("baseline_value")?,
        delta_value: row.try_get("delta_value")?,
        delta_pct: row.try_get("delta_pct")?,
        portfolio_impact_pct: row.try_get("portfolio_impact_pct")?,
        price_used: row.try_get("price_used")?,
        price_source: row.try_get("price_source")?,
        price_updated_at: row.try_get("price_updated_at")?,
        fx_rate_used: row.try_get("fx_rate_used")?,
        fx_source: row.try_get("fx_source")?,
        fx_updated_at: row.try_get("fx_updated_at")?,
        price_stale: row.try_get("price_stale")?,
        fx_stale: row.try_get("fx_stale")?,
        created_at: row.try_get("created_at")?,
    })
}

fn review_from_row(row: sqlx::sqlite::SqliteRow) -> AppResult<DecisionDeltaReview> {
    Ok(DecisionDeltaReview {
        decision_id: row.try_get("decision_id")?,
        notes: row.try_get("notes")?,
        thesis_evidence: serde_json::from_str(&row.try_get::<String, _>("thesis_evidence_json")?)
            .unwrap_or_default(),
        disconfirming_evidence: serde_json::from_str(
            &row.try_get::<String, _>("disconfirming_evidence_json")?,
        )
        .unwrap_or_default(),
        lessons: serde_json::from_str(&row.try_get::<String, _>("lessons_json")?)
            .unwrap_or_default(),
        candidate_principles: serde_json::from_str(
            &row.try_get::<String, _>("candidate_principles_json")?,
        )
        .unwrap_or_default(),
        candidate_checklist_items: serde_json::from_str(
            &row.try_get::<String, _>("candidate_checklist_items_json")?,
        )
        .unwrap_or_default(),
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn positive(value: Option<f64>, field: &str) -> AppResult<f64> {
    match value {
        Some(value) if value.is_finite() && value > 0.0 => Ok(value),
        _ => Err(AppError::bad_request(format!(
            "{field} must be greater than 0"
        ))),
    }
}

fn quantity_from_input(quantity: Option<f64>, notional: Option<f64>, price: f64) -> AppResult<f64> {
    match (quantity, notional) {
        (Some(quantity), _) if quantity.is_finite() && quantity > 0.0 => Ok(quantity),
        (None, Some(notional)) if notional.is_finite() && notional > 0.0 => Ok(notional / price),
        _ => Err(AppError::bad_request("quantity or notional is required")),
    }
}

fn clean_option(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn clean_list(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

fn matching_candidates(selected: Vec<String>, candidates: &[String]) -> Vec<String> {
    selected
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| candidates.iter().any(|candidate| candidate == value))
        .collect()
}

fn dedupe(values: Vec<String>) -> Vec<String> {
    let mut deduped = Vec::new();
    for value in values {
        if !deduped.contains(&value) {
            deduped.push(value);
        }
    }
    deduped
}

fn parse_bool(value: Option<&str>) -> Option<bool> {
    match value?.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" => Some(true),
        "false" | "0" | "no" => Some(false),
        _ => None,
    }
}

fn snapshot_limit(value: Option<usize>) -> usize {
    value
        .filter(|limit| *limit > 0)
        .unwrap_or(DEFAULT_SNAPSHOT_LIMIT)
        .min(MAX_SNAPSHOT_LIMIT)
}

fn round_money(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}
