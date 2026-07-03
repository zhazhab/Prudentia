use std::{
    collections::{HashMap, HashSet},
    fs,
    future::Future,
    io::Cursor,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    extract::{Path, State},
    routing::{get, patch, post},
    Json, Router,
};
use base64::{engine::general_purpose, Engine as _};
use calamine::{Reader, Xlsx};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::{
    ai::runtime::AiRuntime,
    error::{AppError, AppResult},
    market_data::MarketDataProvider,
    state::AppState,
    time::now_iso,
};

const MAX_IMAGE_IMPORT_BYTES: usize = 10 * 1024 * 1024;
const BASE_CURRENCY: &str = "CNY";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioPosition {
    pub symbol: String,
    pub name: String,
    pub asset_type: String,
    pub quantity: f64,
    pub average_cost: f64,
    pub currency: String,
    pub account: Option<String>,
    pub market: Option<String>,
    pub sector: Option<String>,
    pub notes: Option<String>,
    pub last_price: Option<f64>,
    pub market_value: f64,
    pub unrealized_pnl: f64,
    pub weight: f64,
    pub price_updated_at: Option<String>,
    pub price_stale: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PortfolioImportMapping {
    pub symbol: String,
    pub name: String,
    pub quantity: String,
    pub average_cost: String,
    pub currency: String,
    pub account: Option<String>,
    pub market: Option<String>,
    pub sector: Option<String>,
    pub imported_market_value: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PortfolioImportPreviewRequest {
    pub file_name: String,
    pub content: String,
    pub content_encoding: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PortfolioImportPreview {
    pub headers: Vec<String>,
    pub sample_rows: Vec<HashMap<String, String>>,
    pub suggested_mapping: PortfolioImportMapping,
    pub validation_errors: Vec<String>,
    pub draft_rows: Vec<PortfolioDraftRow>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PortfolioImportDraftRequest {
    pub file_name: String,
    pub content: String,
    pub content_encoding: Option<String>,
    pub mapping: PortfolioImportMapping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioDraftRow {
    pub symbol: String,
    pub name: String,
    pub quantity: String,
    pub average_cost: String,
    pub currency: String,
    pub account: Option<String>,
    pub market: String,
    pub sector: Option<String>,
    pub imported_market_value: Option<String>,
    pub notes: Option<String>,
    pub confidence: String,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PortfolioDraftPreview {
    pub draft_rows: Vec<PortfolioDraftRow>,
    pub warnings: Vec<String>,
    pub source: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PortfolioDraftCommitRequest {
    pub rows: Vec<PortfolioDraftRow>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PortfolioImageImportPreviewRequest {
    pub file_name: String,
    pub content: String,
    pub content_encoding: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioImageDraftRow {
    pub symbol: String,
    pub name: String,
    pub quantity: String,
    pub average_cost: String,
    pub currency: String,
    pub account: Option<String>,
    pub market: Option<String>,
    pub sector: Option<String>,
    pub imported_market_value: Option<String>,
    pub notes: Option<String>,
    pub confidence: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioImageRecognition {
    pub rows: Vec<PortfolioImageDraftRow>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioImageImportPreview {
    pub draft_rows: Vec<PortfolioDraftRow>,
    pub rows: Vec<PortfolioImageDraftRow>,
    pub warnings: Vec<String>,
    pub source: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PortfolioImportCommitRequest {
    pub file_name: String,
    pub content: String,
    pub content_encoding: Option<String>,
    pub mapping: PortfolioImportMapping,
}

#[derive(Debug, Clone, Serialize)]
pub struct PortfolioImportResult {
    pub imported_count: usize,
    pub skipped_count: usize,
    pub positions: Vec<PortfolioPosition>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PortfolioSummary {
    pub total_market_value: f64,
    pub total_cost: f64,
    pub total_unrealized_pnl: f64,
    pub positions_count: usize,
    pub price_stale_count: usize,
    pub top_positions: Vec<WeightSlice>,
    pub sectors: Vec<WeightSlice>,
    pub market_groups: Vec<MarketValueGroup>,
    pub base_currency: String,
    pub total_market_value_base: f64,
    pub total_cost_base: f64,
    pub total_unrealized_pnl_base: f64,
    pub fx_rates: Vec<PortfolioFxRate>,
    pub fx_stale_count: usize,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WeightSlice {
    pub label: String,
    pub value: f64,
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MarketValueGroup {
    pub market: String,
    pub currency: String,
    pub market_value: f64,
    pub cost: f64,
    pub unrealized_pnl: f64,
    pub market_value_base: f64,
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PortfolioFxRate {
    pub from_currency: String,
    pub to_currency: String,
    pub rate: f64,
    pub source: String,
    pub updated_at: String,
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PriceRefreshResult {
    pub refreshed: usize,
    pub failed: usize,
    pub failures: Vec<String>,
    pub positions: Vec<PortfolioPosition>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct UpdatePortfolioPositionRequest {
    pub name: Option<String>,
    pub quantity: Option<f64>,
    pub average_cost: Option<f64>,
    pub currency: Option<String>,
    pub account: Option<String>,
    pub market: Option<String>,
    pub sector: Option<String>,
    pub imported_market_value: Option<f64>,
    pub notes: Option<String>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/import/preview", post(preview_import))
        .route("/import/draft", post(draft_import_handler))
        .route("/import/draft/commit", post(commit_draft_handler))
        .route("/import/commit", post(commit_import_handler))
        .route("/positions", get(list_positions_handler))
        .route(
            "/positions/{symbol}",
            patch(update_position_handler).delete(delete_position_handler),
        )
        .route("/summary", get(summary_handler))
        .route("/prices/refresh", post(refresh_prices_handler))
}

pub fn start_price_refresh_job(
    pool: SqlitePool,
    market_data: Arc<dyn MarketDataProvider>,
    interval: Duration,
) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            match refresh_prices(&pool, market_data.clone()).await {
                Ok(result) => tracing::info!(
                    refreshed = result.refreshed,
                    failed = result.failed,
                    "portfolio price refresh finished"
                ),
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
    Ok(Json(commit_import(&state.pool, request).await?))
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

async fn refresh_prices_handler(
    State(state): State<AppState>,
) -> AppResult<Json<PriceRefreshResult>> {
    Ok(Json(
        refresh_prices(&state.pool, state.market_data.clone()).await?,
    ))
}

pub fn preview(request: PortfolioImportPreviewRequest) -> AppResult<PortfolioImportPreview> {
    let table = read_tabular_content(
        &request.file_name,
        &request.content,
        request.content_encoding,
    )?;
    let headers = table.headers;
    let suggested_mapping = suggest_mapping(&headers);
    let mut validation_errors = validate_mapping(&headers, &suggested_mapping);

    if table.rows.is_empty() {
        validation_errors.push("file has no data rows".to_string());
    }

    let sample_rows = table
        .rows
        .iter()
        .take(8)
        .map(|row| row_to_map(&headers, row))
        .collect();
    let draft_rows = draft_rows_from_table(&headers, &table.rows, &suggested_mapping);

    Ok(PortfolioImportPreview {
        headers,
        sample_rows,
        suggested_mapping,
        validation_errors,
        draft_rows,
    })
}

pub fn draft_from_import(request: PortfolioImportDraftRequest) -> AppResult<PortfolioDraftPreview> {
    let table = read_tabular_content(
        &request.file_name,
        &request.content,
        request.content_encoding,
    )?;
    let mut warnings = validate_mapping(&table.headers, &request.mapping);
    if table.rows.is_empty() {
        warnings.push("file has no data rows".to_string());
    }

    Ok(PortfolioDraftPreview {
        draft_rows: draft_rows_from_table(&table.headers, &table.rows, &request.mapping),
        warnings,
        source: "file".to_string(),
    })
}

pub async fn preview_image_import(
    ai: Arc<AiRuntime>,
    request: PortfolioImageImportPreviewRequest,
) -> AppResult<PortfolioImageImportPreview> {
    preview_image_import_with_progress(ai, request, |_| async {}).await
}

pub async fn preview_image_import_with_progress<F, Fut>(
    ai: Arc<AiRuntime>,
    request: PortfolioImageImportPreviewRequest,
    mut progress: F,
) -> AppResult<PortfolioImageImportPreview>
where
    F: FnMut(&'static str) -> Fut,
    Fut: Future<Output = ()>,
{
    let started_at = Instant::now();
    tracing::info!(
        file_name = %request.file_name,
        mime_type = request.mime_type.as_deref().unwrap_or("unknown"),
        "portfolio image import preview started"
    );
    progress("validating_image").await;

    if !matches!(request.content_encoding.as_deref(), Some("base64")) {
        return Err(AppError::bad_request(
            "image imports must send content_encoding=base64",
        ));
    }

    let extension = supported_image_extension(&request.file_name, request.mime_type.as_deref())
        .ok_or_else(|| AppError::bad_request("unsupported image type"))?;
    let bytes = general_purpose::STANDARD.decode(request.content.trim())?;
    if bytes.is_empty() {
        return Err(AppError::bad_request("image content is empty"));
    }
    if bytes.len() > MAX_IMAGE_IMPORT_BYTES {
        return Err(AppError::bad_request("image content is too large"));
    }
    tracing::info!(
        file_name = %request.file_name,
        mime_type = request.mime_type.as_deref().unwrap_or("unknown"),
        extension,
        image_bytes = bytes.len(),
        "portfolio image import payload validated"
    );

    progress("writing_temp_image").await;
    let temp_image = TemporaryImportFile::write("prudentia-portfolio-image", extension, &bytes)?;
    progress("recognizing_image").await;
    let recognition_started_at = Instant::now();
    let recognition = ai
        .recognize_portfolio_image(&temp_image.path)
        .await
        .map_err(|err| AppError::internal(err.to_string()))?;
    tracing::info!(
        file_name = %request.file_name,
        elapsed_ms = recognition_started_at.elapsed().as_millis(),
        recognized_rows = recognition.rows.len(),
        recognition_warnings = recognition.warnings.len(),
        "portfolio image recognition provider returned"
    );
    progress("normalizing_rows").await;
    let mut warnings = recognition.warnings;
    if recognition.rows.is_empty() && warnings.is_empty() {
        warnings.push("No visible holding rows were recognized.".to_string());
    }

    let rows = recognition
        .rows
        .into_iter()
        .map(clean_image_draft_row)
        .collect::<Vec<_>>();
    let draft_rows = rows
        .iter()
        .cloned()
        .map(draft_row_from_image_row)
        .collect::<Vec<_>>();
    let row_count = draft_rows.len();
    let error_count = draft_rows.iter().map(|row| row.errors.len()).sum::<usize>();
    tracing::info!(
        file_name = %request.file_name,
        elapsed_ms = started_at.elapsed().as_millis(),
        row_count,
        warning_count = warnings.len(),
        error_count,
        "portfolio image import preview normalized"
    );

    Ok(PortfolioImageImportPreview {
        draft_rows,
        rows,
        warnings,
        source: "codex_cli".to_string(),
    })
}

pub async fn commit_import(
    pool: &SqlitePool,
    request: PortfolioImportCommitRequest,
) -> AppResult<PortfolioImportResult> {
    let table = read_tabular_content(
        &request.file_name,
        &request.content,
        request.content_encoding,
    )?;
    validate_mapping(&table.headers, &request.mapping)
        .into_iter()
        .next()
        .map_or(Ok(()), |message| Err(AppError::bad_request(message)))?;

    let mut imported = Vec::new();
    let mut skipped_count = 0;

    for (index, row) in table.rows.iter().enumerate() {
        match position_from_row(&table.headers, row, &request.mapping) {
            Ok(position) => {
                upsert_position(pool, &position).await?;
                imported.push(position);
            }
            Err(error) => {
                skipped_count += 1;
                tracing::warn!(row = index + 2, error = ?error, "skipping invalid portfolio row");
            }
        }
    }

    recompute_weights(pool).await?;

    Ok(PortfolioImportResult {
        imported_count: imported.len(),
        skipped_count,
        positions: list_positions(pool).await?,
    })
}

pub async fn commit_draft_rows(
    pool: &SqlitePool,
    market_data: Arc<dyn MarketDataProvider>,
    request: PortfolioDraftCommitRequest,
) -> AppResult<PortfolioImportResult> {
    if request.rows.is_empty() {
        return Err(AppError::bad_request("draft has no rows"));
    }

    let mut validation_errors = Vec::new();
    let mut positions = Vec::new();
    let mut seen_symbols = HashSet::new();
    for (index, row) in request.rows.into_iter().enumerate() {
        let row = normalize_draft_row(row);
        let errors = validate_draft_row(&row);
        if errors.is_empty() {
            let position = position_from_draft_row(&row)?;
            if !seen_symbols.insert(position.symbol.clone()) {
                validation_errors.push(format!(
                    "row {}: duplicate symbol {}",
                    index + 1,
                    position.symbol
                ));
            } else {
                positions.push(position);
            }
        } else {
            validation_errors.push(format!("row {}: {}", index + 1, errors.join("; ")));
        }
    }

    if !validation_errors.is_empty() {
        return Err(AppError::bad_request(validation_errors.join(" ")));
    }

    for position in &positions {
        upsert_position(pool, position).await?;
    }

    recompute_weights_with_fx(pool, market_data).await?;

    Ok(PortfolioImportResult {
        imported_count: positions.len(),
        skipped_count: 0,
        positions: list_positions(pool).await?,
    })
}

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
    recompute_weights_with_fx(pool, market_data).await?;
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
    recompute_weights_with_fx(pool, market_data).await?;
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
            }
        }
    }

    if let Err(error) = recompute_weights_with_fx(pool, market_data).await {
        tracing::warn!(error = ?error, "falling back to native portfolio weights after FX refresh failed");
        recompute_weights(pool).await?;
    }

    Ok(PriceRefreshResult {
        refreshed,
        failed,
        failures,
        positions: list_positions(pool).await?,
    })
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

fn read_tabular_content(
    file_name: &str,
    content: &str,
    content_encoding: Option<String>,
) -> AppResult<TabularContent> {
    if file_name.ends_with(".xlsx") {
        return read_xlsx(content, content_encoding);
    }

    let bytes = if matches!(content_encoding.as_deref(), Some("base64")) {
        general_purpose::STANDARD.decode(content)?
    } else {
        content.as_bytes().to_vec()
    };
    let plain = String::from_utf8(bytes)
        .map_err(|_| AppError::bad_request("import content must be valid UTF-8"))?;
    read_delimited(file_name, &plain)
}

fn read_delimited(file_name: &str, content: &str) -> AppResult<TabularContent> {
    let delimiter = if file_name.ends_with(".tsv") {
        b'\t'
    } else {
        b','
    };
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .delimiter(delimiter)
        .from_reader(content.as_bytes());
    let headers = reader
        .headers()?
        .iter()
        .map(|value| value.trim().to_string())
        .collect::<Vec<_>>();
    let mut rows = Vec::new();

    for record in reader.records() {
        rows.push(
            record?
                .iter()
                .map(|value| value.trim().to_string())
                .collect::<Vec<_>>(),
        );
    }

    Ok(TabularContent { headers, rows })
}

fn read_xlsx(content: &str, content_encoding: Option<String>) -> AppResult<TabularContent> {
    if !matches!(content_encoding.as_deref(), Some("base64")) {
        return Err(AppError::bad_request(
            "xlsx imports must send content_encoding=base64",
        ));
    }

    let bytes = general_purpose::STANDARD.decode(content)?;
    let cursor = Cursor::new(bytes);
    let mut workbook: Xlsx<_> =
        Xlsx::new(cursor).map_err(|err| AppError::bad_request(err.to_string()))?;
    let range = workbook
        .worksheet_range_at(0)
        .ok_or_else(|| AppError::bad_request("xlsx workbook has no worksheets"))?
        .map_err(|err| AppError::bad_request(err.to_string()))?;
    let mut rows = range.rows().map(cells_to_strings).collect::<Vec<_>>();

    if rows.is_empty() {
        return Ok(TabularContent {
            headers: Vec::new(),
            rows: Vec::new(),
        });
    }

    let headers = rows.remove(0);
    Ok(TabularContent { headers, rows })
}

fn cells_to_strings(row: &[calamine::Data]) -> Vec<String> {
    row.iter().map(|cell| cell.to_string()).collect()
}

fn draft_rows_from_table(
    headers: &[String],
    rows: &[Vec<String>],
    mapping: &PortfolioImportMapping,
) -> Vec<PortfolioDraftRow> {
    rows.iter()
        .map(|row| draft_row_from_table_row(headers, row, mapping))
        .collect()
}

fn draft_row_from_table_row(
    headers: &[String],
    row: &[String],
    mapping: &PortfolioImportMapping,
) -> PortfolioDraftRow {
    let symbol = required_or_empty(headers, row, &mapping.symbol).to_ascii_uppercase();
    let mapped_market = optional_cell(headers, row, mapping.market.as_deref())
        .map(|value| normalize_market(&value));
    let inferred_market = mapped_market.or_else(|| infer_market(&symbol));
    let mapped_currency = required_or_empty(headers, row, &mapping.currency).to_ascii_uppercase();
    let currency = if mapped_currency.is_empty() {
        inferred_currency(&symbol, inferred_market.as_deref()).unwrap_or_default()
    } else {
        mapped_currency
    };
    let market = inferred_market.unwrap_or_else(|| "Other".to_string());

    normalize_and_validate_draft_row(PortfolioDraftRow {
        symbol,
        name: required_or_empty(headers, row, &mapping.name),
        quantity: required_or_empty(headers, row, &mapping.quantity),
        average_cost: required_or_empty(headers, row, &mapping.average_cost),
        currency,
        account: optional_cell(headers, row, mapping.account.as_deref()),
        market,
        sector: optional_cell(headers, row, mapping.sector.as_deref()),
        imported_market_value: optional_cell(
            headers,
            row,
            mapping.imported_market_value.as_deref(),
        ),
        notes: optional_cell(headers, row, mapping.notes.as_deref()),
        confidence: "high".to_string(),
        warnings: Vec::new(),
        errors: Vec::new(),
    })
}

fn draft_row_from_image_row(row: PortfolioImageDraftRow) -> PortfolioDraftRow {
    let symbol = row.symbol.to_ascii_uppercase();
    let market = row
        .market
        .as_deref()
        .map(normalize_market)
        .or_else(|| infer_market(&symbol))
        .unwrap_or_else(|| "Other".to_string());
    let currency = if row.currency.trim().is_empty() {
        inferred_currency(&symbol, Some(&market)).unwrap_or_default()
    } else {
        row.currency.to_ascii_uppercase()
    };

    normalize_and_validate_draft_row(PortfolioDraftRow {
        symbol,
        name: row.name,
        quantity: row.quantity,
        average_cost: row.average_cost,
        currency,
        account: row.account,
        market,
        sector: row.sector,
        imported_market_value: row.imported_market_value,
        notes: row.notes,
        confidence: row.confidence,
        warnings: row.warnings,
        errors: Vec::new(),
    })
}

fn normalize_and_validate_draft_row(row: PortfolioDraftRow) -> PortfolioDraftRow {
    let mut row = normalize_draft_row(row);
    row.errors = validate_draft_row(&row);
    row
}

fn normalize_draft_row(mut row: PortfolioDraftRow) -> PortfolioDraftRow {
    row.symbol = row.symbol.trim().to_ascii_uppercase();
    row.name = row.name.trim().to_string();
    row.quantity = row.quantity.trim().to_string();
    row.average_cost = row.average_cost.trim().to_string();
    row.currency = row.currency.trim().to_ascii_uppercase();
    row.account = clean_optional_string(row.account);
    row.market = normalize_market(&row.market);
    row.sector = clean_optional_string(row.sector);
    row.imported_market_value = clean_optional_string(row.imported_market_value);
    row.notes = clean_optional_string(row.notes);
    row.confidence = match row.confidence.trim().to_ascii_lowercase().as_str() {
        "high" | "medium" | "low" | "unknown" => row.confidence.trim().to_ascii_lowercase(),
        _ => "unknown".to_string(),
    };
    row.warnings = row
        .warnings
        .into_iter()
        .map(|warning| warning.trim().to_string())
        .filter(|warning| !warning.is_empty())
        .collect();
    row.errors = row
        .errors
        .into_iter()
        .map(|error| error.trim().to_string())
        .filter(|error| !error.is_empty())
        .collect();
    row
}

fn validate_draft_row(row: &PortfolioDraftRow) -> Vec<String> {
    let mut errors = Vec::new();
    if row.symbol.trim().is_empty() {
        errors.push("symbol is required".to_string());
    }
    if row.name.trim().is_empty() {
        errors.push("name is required".to_string());
    }
    match parse_positive_f64(&row.quantity, "quantity") {
        Ok(_) => {}
        Err(error) => errors.push(error.to_string()),
    }
    match parse_non_negative_f64(&row.average_cost, "average_cost") {
        Ok(_) => {}
        Err(error) => errors.push(error.to_string()),
    }
    if row.currency.trim().is_empty() {
        errors.push("currency is required".to_string());
    }
    if row.market.trim().is_empty() {
        errors.push("market is required".to_string());
    }
    if let Some(imported_market_value) = &row.imported_market_value {
        if let Err(error) = parse_non_negative_f64(imported_market_value, "imported_market_value") {
            errors.push(error.to_string());
        }
    }
    errors
}

fn position_from_draft_row(row: &PortfolioDraftRow) -> AppResult<PortfolioPosition> {
    let quantity = parse_positive_f64(&row.quantity, "quantity")?;
    let average_cost = parse_non_negative_f64(&row.average_cost, "average_cost")?;
    let imported_market_value = row
        .imported_market_value
        .as_deref()
        .and_then(|value| parse_non_negative_f64(value, "imported_market_value").ok());
    let last_price = imported_market_value
        .map(|value| ratio(value, quantity))
        .or(Some(average_cost));
    let market_value = imported_market_value.unwrap_or(quantity * average_cost);
    let cost_basis = quantity * average_cost;

    Ok(PortfolioPosition {
        symbol: row.symbol.to_ascii_uppercase(),
        name: row.name.clone(),
        asset_type: "stock".to_string(),
        quantity,
        average_cost,
        currency: row.currency.to_ascii_uppercase(),
        account: row.account.clone(),
        market: Some(normalize_market(&row.market)),
        sector: row.sector.clone(),
        notes: row.notes.clone(),
        last_price,
        market_value,
        unrealized_pnl: market_value - cost_basis,
        weight: 0.0,
        price_updated_at: None,
        price_stale: true,
        updated_at: now_iso(),
    })
}

fn position_from_row(
    headers: &[String],
    row: &[String],
    mapping: &PortfolioImportMapping,
) -> AppResult<PortfolioPosition> {
    let symbol = required_cell(headers, row, &mapping.symbol)?.to_uppercase();
    let name = required_cell(headers, row, &mapping.name)?;
    let quantity =
        parse_positive_f64(&required_cell(headers, row, &mapping.quantity)?, "quantity")?;
    let average_cost = parse_non_negative_f64(
        &required_cell(headers, row, &mapping.average_cost)?,
        "average_cost",
    )?;
    let currency = required_cell(headers, row, &mapping.currency)?.to_uppercase();
    let imported_market_value =
        optional_cell(headers, row, mapping.imported_market_value.as_deref())
            .and_then(|value| parse_non_negative_f64(&value, "imported_market_value").ok());
    let last_price = imported_market_value
        .map(|value| ratio(value, quantity))
        .or(Some(average_cost));
    let market_value = imported_market_value.unwrap_or(quantity * average_cost);
    let cost_basis = quantity * average_cost;

    Ok(PortfolioPosition {
        symbol,
        name,
        asset_type: "stock".to_string(),
        quantity,
        average_cost,
        currency,
        account: optional_cell(headers, row, mapping.account.as_deref()),
        market: optional_cell(headers, row, mapping.market.as_deref()),
        sector: optional_cell(headers, row, mapping.sector.as_deref()),
        notes: optional_cell(headers, row, mapping.notes.as_deref()),
        last_price,
        market_value,
        unrealized_pnl: market_value - cost_basis,
        weight: 0.0,
        price_updated_at: None,
        price_stale: true,
        updated_at: now_iso(),
    })
}

fn validate_mapping(headers: &[String], mapping: &PortfolioImportMapping) -> Vec<String> {
    let mut errors = Vec::new();
    for (field, header) in [
        ("symbol", &mapping.symbol),
        ("name", &mapping.name),
        ("quantity", &mapping.quantity),
        ("average_cost", &mapping.average_cost),
        ("currency", &mapping.currency),
    ] {
        if header.trim().is_empty() {
            errors.push(format!("{field} mapping is required"));
        } else if column_index(headers, header).is_none() {
            errors.push(format!(
                "{field} mapping points to missing column '{header}'"
            ));
        }
    }
    errors
}

fn suggest_mapping(headers: &[String]) -> PortfolioImportMapping {
    PortfolioImportMapping {
        symbol: find_header(headers, &["symbol", "ticker", "代码", "证券代码"]).unwrap_or_default(),
        name: find_header(headers, &["name", "security", "证券名称", "名称"]).unwrap_or_default(),
        quantity: find_header(headers, &["quantity", "shares", "持仓", "数量"]).unwrap_or_default(),
        average_cost: find_header(
            headers,
            &["average cost", "avg cost", "cost", "成本", "成本价"],
        )
        .unwrap_or_default(),
        currency: find_header(headers, &["currency", "币种"]).unwrap_or_default(),
        account: find_header(headers, &["account", "账户"]),
        market: find_header(headers, &["market", "exchange", "市场"]),
        sector: find_header(headers, &["sector", "行业"]),
        imported_market_value: find_header(headers, &["market value", "市值"]),
        notes: find_header(headers, &["notes", "备注"]),
    }
}

fn find_header(headers: &[String], candidates: &[&str]) -> Option<String> {
    headers.iter().find_map(|header| {
        let normalized = normalize_header(header);
        candidates
            .iter()
            .any(|candidate| normalized == normalize_header(candidate))
            .then(|| header.clone())
    })
}

fn required_cell(headers: &[String], row: &[String], header: &str) -> AppResult<String> {
    optional_cell(headers, row, Some(header))
        .ok_or_else(|| AppError::bad_request(format!("missing required value for {header}")))
}

fn required_or_empty(headers: &[String], row: &[String], header: &str) -> String {
    optional_cell(headers, row, Some(header)).unwrap_or_default()
}

fn optional_cell(headers: &[String], row: &[String], header: Option<&str>) -> Option<String> {
    let header = header?;
    let index = column_index(headers, header)?;
    row.get(index)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn column_index(headers: &[String], header: &str) -> Option<usize> {
    let target = normalize_header(header);
    headers
        .iter()
        .position(|candidate| normalize_header(candidate) == target)
}

fn row_to_map(headers: &[String], row: &[String]) -> HashMap<String, String> {
    headers
        .iter()
        .enumerate()
        .map(|(index, header)| (header.clone(), row.get(index).cloned().unwrap_or_default()))
        .collect()
}

fn parse_positive_f64(value: &str, field: &str) -> AppResult<f64> {
    let normalized = value.replace(',', "");
    let parsed = normalized
        .parse::<f64>()
        .map_err(|_| AppError::bad_request(format!("{field} must be a number")))?;
    if parsed <= 0.0 {
        return Err(AppError::bad_request(format!(
            "{field} must be greater than 0"
        )));
    }
    Ok(parsed)
}

fn parse_non_negative_f64(value: &str, field: &str) -> AppResult<f64> {
    let normalized = value.replace(',', "");
    let parsed = normalized
        .parse::<f64>()
        .map_err(|_| AppError::bad_request(format!("{field} must be a number")))?;
    if parsed < 0.0 {
        return Err(AppError::bad_request(format!(
            "{field} must be non-negative"
        )));
    }
    Ok(parsed)
}

fn position_from_db_row(row: sqlx::sqlite::SqliteRow) -> AppResult<PortfolioPosition> {
    Ok(PortfolioPosition {
        symbol: row.try_get("symbol")?,
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
        market_value: row.try_get("market_value")?,
        unrealized_pnl: row.try_get("unrealized_pnl")?,
        weight: row.try_get("weight")?,
        price_updated_at: row.try_get("price_updated_at")?,
        price_stale: row.try_get::<i64, _>("price_stale")? != 0,
        updated_at: row.try_get("updated_at")?,
    })
}

fn ratio(value: f64, denominator: f64) -> f64 {
    if denominator.abs() < f64::EPSILON {
        0.0
    } else {
        value / denominator
    }
}

fn normalize_header(value: &str) -> String {
    value.trim().to_lowercase().replace([' ', '_', '-'], "")
}

fn normalize_market(value: &str) -> String {
    match value.trim().to_ascii_uppercase().as_str() {
        "US" | "USA" | "NYSE" | "NASDAQ" => "US".to_string(),
        "HK" | "HKG" | "HKEX" | "香港" => "HK".to_string(),
        "CN" | "CHINA" | "SH" | "SHANGHAI" | "SZ" | "SHENZHEN" | "沪深" | "A股" => {
            "CN".to_string()
        }
        "OTHER" | "其他" => "Other".to_string(),
        other if other.is_empty() => String::new(),
        other => other.to_string(),
    }
}

fn infer_market(symbol: &str) -> Option<String> {
    let symbol = symbol.trim().to_ascii_uppercase();
    if symbol.is_empty() {
        return None;
    }
    if symbol.ends_with(".HK")
        || (symbol.chars().all(|value| value.is_ascii_digit()) && symbol.len() <= 5)
    {
        return Some("HK".to_string());
    }
    if symbol.ends_with(".SS")
        || symbol.ends_with(".SH")
        || symbol.ends_with(".SHH")
        || symbol.ends_with(".SZ")
        || symbol.ends_with(".SHE")
        || symbol.ends_with(".SHZ")
        || (symbol.len() == 6
            && symbol.chars().all(|value| value.is_ascii_digit())
            && matches!(symbol.as_bytes()[0], b'0' | b'3' | b'6'))
    {
        return Some("CN".to_string());
    }
    if symbol
        .chars()
        .all(|value| value.is_ascii_alphabetic() || value == '.')
    {
        return Some("US".to_string());
    }
    None
}

fn inferred_currency(symbol: &str, market: Option<&str>) -> Option<String> {
    match market.map(normalize_market).as_deref() {
        Some("US") => Some("USD".to_string()),
        Some("HK") => Some("HKD".to_string()),
        Some("CN") => Some("CNY".to_string()),
        _ => infer_market(symbol).and_then(|market| inferred_currency("", Some(&market))),
    }
}

fn supported_image_extension(file_name: &str, mime_type: Option<&str>) -> Option<&'static str> {
    match mime_type.map(|value| value.trim().to_ascii_lowercase()) {
        Some(value) if value == "image/png" => return Some("png"),
        Some(value) if value == "image/jpeg" || value == "image/jpg" => return Some("jpg"),
        Some(value) if value == "image/webp" => return Some("webp"),
        Some(value) if !value.is_empty() => return None,
        _ => {}
    }

    let lower_name = file_name.trim().to_ascii_lowercase();
    if lower_name.ends_with(".png") {
        Some("png")
    } else if lower_name.ends_with(".jpg") || lower_name.ends_with(".jpeg") {
        Some("jpg")
    } else if lower_name.ends_with(".webp") {
        Some("webp")
    } else {
        None
    }
}

fn clean_image_draft_row(mut row: PortfolioImageDraftRow) -> PortfolioImageDraftRow {
    row.symbol = row.symbol.trim().to_ascii_uppercase();
    row.name = row.name.trim().to_string();
    row.quantity = row.quantity.trim().to_string();
    row.average_cost = row.average_cost.trim().to_string();
    row.currency = row.currency.trim().to_ascii_uppercase();
    row.account = clean_optional_string(row.account);
    row.market = clean_optional_string(row.market);
    row.sector = clean_optional_string(row.sector);
    row.imported_market_value = clean_optional_string(row.imported_market_value);
    row.notes = clean_optional_string(row.notes);
    row.confidence = match row.confidence.trim().to_ascii_lowercase().as_str() {
        "high" | "medium" | "low" | "unknown" => row.confidence.trim().to_ascii_lowercase(),
        _ => "unknown".to_string(),
    };
    row.warnings = row
        .warnings
        .into_iter()
        .map(|warning| warning.trim().to_string())
        .filter(|warning| !warning.is_empty())
        .collect();
    row
}

fn clean_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(clean_string)
}

fn clean_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

struct TemporaryImportFile {
    path: PathBuf,
}

impl TemporaryImportFile {
    fn write(prefix: &str, extension: &str, bytes: &[u8]) -> AppResult<Self> {
        let file_name = format!("{prefix}-{}.{}", Uuid::new_v4(), extension);
        let path = std::env::temp_dir().join(file_name);
        fs::write(&path, bytes)
            .map_err(|err| AppError::internal(format!("failed to write temporary image: {err}")))?;
        Ok(Self { path })
    }
}

impl Drop for TemporaryImportFile {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_file(&self.path) {
            tracing::debug!(path = %self.path.display(), error = %error, "temporary image cleanup failed");
        }
    }
}

struct TabularContent {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}
