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
