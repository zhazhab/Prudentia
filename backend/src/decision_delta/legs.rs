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
                leg(LegDraft {
                    decision_id,
                    leg_kind: "actual",
                    baseline_type: None,
                    symbol: Some(symbol),
                    quantity: Some(quantity),
                    notional: None,
                    price: Some(price),
                    currency: &currency,
                    now: &now,
                }),
                leg(LegDraft {
                    decision_id,
                    leg_kind: "baseline",
                    baseline_type: Some(input.baseline_type.unwrap_or_else(|| "cash".to_string())),
                    symbol: None,
                    quantity: None,
                    notional: Some(notional),
                    price: None,
                    currency: &currency,
                    now: &now,
                }),
            ])
        }
        "sell" | "trim" => {
            let symbol = symbol.ok_or_else(|| AppError::bad_request("symbol is required"))?;
            let price = positive(input.price, "price")?;
            let quantity = positive(input.quantity, "quantity")?;
            let notional = input.notional.unwrap_or(quantity * price);
            Ok(vec![
                leg(LegDraft {
                    decision_id,
                    leg_kind: "actual",
                    baseline_type: Some("cash".to_string()),
                    symbol: None,
                    quantity: None,
                    notional: Some(notional),
                    price: None,
                    currency: &currency,
                    now: &now,
                }),
                leg(LegDraft {
                    decision_id,
                    leg_kind: "baseline",
                    baseline_type: Some(
                        input
                            .baseline_type
                            .unwrap_or_else(|| "continue_holding".to_string()),
                    ),
                    symbol: Some(symbol),
                    quantity: Some(quantity),
                    notional: None,
                    price: Some(price),
                    currency: &currency,
                    now: &now,
                }),
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
                leg(LegDraft {
                    decision_id,
                    leg_kind: "actual",
                    baseline_type: Some("cash".to_string()),
                    symbol: None,
                    quantity: None,
                    notional: Some(notional),
                    price: None,
                    currency: &currency,
                    now: &now,
                }),
                leg(LegDraft {
                    decision_id,
                    leg_kind: "baseline",
                    baseline_type: Some(
                        input
                            .baseline_type
                            .unwrap_or_else(|| "hypothetical_buy".to_string()),
                    ),
                    symbol: Some(symbol),
                    quantity: Some(quantity),
                    notional: None,
                    price: Some(price),
                    currency: &currency,
                    now: &now,
                }),
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

struct LegDraft<'a> {
    decision_id: &'a str,
    leg_kind: &'a str,
    baseline_type: Option<String>,
    symbol: Option<String>,
    quantity: Option<f64>,
    notional: Option<f64>,
    price: Option<f64>,
    currency: &'a str,
    now: &'a str,
}

fn leg(draft: LegDraft<'_>) -> DecisionDeltaLeg {
    DecisionDeltaLeg {
        id: Uuid::new_v4().to_string(),
        decision_id: draft.decision_id.to_string(),
        leg_kind: draft.leg_kind.to_string(),
        baseline_type: draft.baseline_type,
        symbol: draft.symbol,
        quantity: draft.quantity,
        notional: draft.notional,
        price: draft.price,
        currency: draft.currency.to_ascii_uppercase(),
        created_at: draft.now.to_string(),
        updated_at: draft.now.to_string(),
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
