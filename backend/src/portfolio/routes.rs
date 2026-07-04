pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/import/preview", post(preview_import))
        .route("/import/draft", post(draft_import_handler))
        .route("/import/draft/commit", post(commit_draft_handler))
        .route("/import/commit", post(commit_import_handler))
        .route("/symbols/search", get(search_symbols_handler))
        .route("/symbols/refresh", post(refresh_symbols_handler))
        .route("/symbols/resolve-draft", post(resolve_draft_symbols_handler))
        .route("/positions", get(list_positions_handler))
        .route(
            "/positions/{symbol}",
            patch(update_position_handler).delete(delete_position_handler),
        )
        .route("/summary", get(summary_handler))
        .route("/performance", get(performance_handler))
        .route("/prices/refresh", post(refresh_prices_handler))
}

pub fn start_price_refresh_job(
    pool: SqlitePool,
    market_data: Arc<dyn MarketDataProvider>,
    interval: Duration,
    ttl: Duration,
) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            match refresh_prices_if_due(&pool, market_data.clone(), ttl).await {
                Ok(Some(result)) => tracing::info!(
                    refreshed = result.refreshed,
                    failed = result.failed,
                    "portfolio daily price refresh finished"
                ),
                Ok(None) => tracing::debug!("portfolio daily price refresh skipped"),
                Err(error) => tracing::warn!(error = ?error, "portfolio price refresh failed"),
            }
        }
    });
}

async fn preview_import(
    Json(request): Json<PortfolioImportPreviewRequest>,
) -> AppResult<Json<PortfolioImportPreview>> {
    Ok(Json(preview(request)?))
}

async fn draft_import_handler(
    Json(request): Json<PortfolioImportDraftRequest>,
) -> AppResult<Json<PortfolioDraftPreview>> {
    Ok(Json(draft_from_import(request)?))
}

async fn commit_import_handler(
    State(state): State<AppState>,
    Json(request): Json<PortfolioImportCommitRequest>,
) -> AppResult<Json<PortfolioImportResult>> {
    Ok(Json(
        commit_import(&state.pool, state.market_data.clone(), request).await?,
    ))
}

async fn commit_draft_handler(
    State(state): State<AppState>,
    Json(request): Json<PortfolioDraftCommitRequest>,
) -> AppResult<Json<PortfolioImportResult>> {
    Ok(Json(
        commit_draft_rows(&state.pool, state.market_data.clone(), request).await?,
    ))
}

async fn list_positions_handler(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<PortfolioPosition>>> {
    Ok(Json(list_positions(&state.pool).await?))
}

async fn update_position_handler(
    State(state): State<AppState>,
    Path(symbol): Path<String>,
    Json(request): Json<UpdatePortfolioPositionRequest>,
) -> AppResult<Json<PortfolioPosition>> {
    Ok(Json(
        update_position(&state.pool, state.market_data.clone(), &symbol, request).await?,
    ))
}

async fn delete_position_handler(
    State(state): State<AppState>,
    Path(symbol): Path<String>,
) -> AppResult<Json<Vec<PortfolioPosition>>> {
    delete_position(&state.pool, state.market_data.clone(), &symbol).await?;
    Ok(Json(list_positions(&state.pool).await?))
}

async fn summary_handler(State(state): State<AppState>) -> AppResult<Json<PortfolioSummary>> {
    Ok(Json(
        summary_with_fx(&state.pool, state.market_data.clone()).await?,
    ))
}

async fn performance_handler(
    State(state): State<AppState>,
    Query(query): Query<PortfolioPerformanceQuery>,
) -> AppResult<Json<PortfolioPerformanceResponse>> {
    Ok(Json(portfolio_performance(&state.pool, query).await?))
}

async fn refresh_prices_handler(
    State(state): State<AppState>,
) -> AppResult<Json<PriceRefreshResult>> {
    Ok(Json(
        refresh_prices(&state.pool, state.market_data.clone()).await?,
    ))
}

async fn search_symbols_handler(
    State(state): State<AppState>,
    Query(query): Query<SecuritySymbolSearchQuery>,
) -> AppResult<Json<Vec<SecuritySymbol>>> {
    Ok(Json(search_security_symbols(&state.pool, &query).await?))
}

async fn refresh_symbols_handler(
    State(state): State<AppState>,
) -> AppResult<Json<SecuritySymbolRefreshResult>> {
    Ok(Json(refresh_security_symbols_from_config(&state.pool).await?))
}

async fn resolve_draft_symbols_handler(
    State(state): State<AppState>,
    Json(request): Json<PortfolioDraftSymbolResolveRequest>,
) -> AppResult<Json<PortfolioDraftSymbolResolveResult>> {
    Ok(Json(
        resolve_draft_symbols_from_directory(&state.pool, request).await?,
    ))
}
