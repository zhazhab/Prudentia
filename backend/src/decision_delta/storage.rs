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
