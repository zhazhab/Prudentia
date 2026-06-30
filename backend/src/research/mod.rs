use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};
use uuid::Uuid;

use crate::{
    ai::{runtime::AiRuntime, PortfolioReviewContext, ResearchSourceInput, StockSnapshotContext},
    error::{AppError, AppResult},
    investment_system::{self, InvestmentSystem, UpdateInvestmentSystemRequest},
    locale::Locale,
    market_data::MarketDataProvider,
    memo, portfolio,
    state::AppState,
    time::now_iso,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchRecordKind {
    Distillation,
    StockSnapshot,
    PortfolioReview,
}

impl ResearchRecordKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Distillation => "distillation",
            Self::StockSnapshot => "stock_snapshot",
            Self::PortfolioReview => "portfolio_review",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "distillation" => Some(Self::Distillation),
            "stock_snapshot" => Some(Self::StockSnapshot),
            "portfolio_review" => Some(Self::PortfolioReview),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchAnalysis {
    pub summary: String,
    pub insights: Vec<String>,
    pub risks: Vec<String>,
    pub checklist: Vec<String>,
    pub candidate_principles: Vec<String>,
    pub candidate_checklist_items: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchRecord {
    pub id: String,
    pub kind: ResearchRecordKind,
    pub title: String,
    pub source_type: Option<String>,
    pub source_title: Option<String>,
    pub source_author: Option<String>,
    pub source_content: Option<String>,
    pub symbol: Option<String>,
    pub memo_id: Option<String>,
    pub summary: String,
    pub insights: Vec<String>,
    pub risks: Vec<String>,
    pub checklist: Vec<String>,
    pub candidate_principles: Vec<String>,
    pub candidate_checklist_items: Vec<String>,
    pub raw_output: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateResearchRecord {
    pub kind: ResearchRecordKind,
    pub title: String,
    pub source_type: Option<String>,
    pub source_title: Option<String>,
    pub source_author: Option<String>,
    pub source_content: Option<String>,
    pub symbol: Option<String>,
    pub memo_id: Option<String>,
    pub analysis: ResearchAnalysis,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DistillResearchRequest {
    pub title: String,
    pub source_type: Option<String>,
    pub source_title: Option<String>,
    pub source_author: Option<String>,
    pub source_content: String,
    pub symbol: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StockSnapshotRequest {
    pub symbol: String,
    pub memo_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdoptResearchCandidatesRequest {
    pub principles: Vec<String>,
    pub checklist_items: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResearchRecordListParams {
    pub kind: Option<String>,
    pub symbol: Option<String>,
    pub q: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ResearchRecordQuery {
    pub kind: Option<ResearchRecordKind>,
    pub symbol: Option<String>,
    pub q: Option<String>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/records", get(list_records_handler))
        .route("/records/{id}", get(get_record_handler))
        .route("/records/{id}/adopt", post(adopt_candidates_handler))
        .route("/distill", post(distill_handler))
        .route("/stock-snapshot", post(stock_snapshot_handler))
        .route("/portfolio-review", post(portfolio_review_handler))
}

async fn list_records_handler(
    State(state): State<AppState>,
    Query(params): Query<ResearchRecordListParams>,
) -> AppResult<Json<Vec<ResearchRecord>>> {
    let query = ResearchRecordQuery {
        kind: params
            .kind
            .as_deref()
            .map(|kind| {
                ResearchRecordKind::parse(kind)
                    .ok_or_else(|| AppError::bad_request("invalid research record kind"))
            })
            .transpose()?,
        symbol: clean_option(params.symbol),
        q: clean_option(params.q),
    };
    Ok(Json(list_records(&state.pool, query).await?))
}

async fn get_record_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ResearchRecord>> {
    Ok(Json(get_record(&state.pool, &id).await?))
}

async fn distill_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<DistillResearchRequest>,
) -> AppResult<Json<ResearchRecord>> {
    Ok(Json(
        distill(
            &state.pool,
            state.ai.clone(),
            request,
            Locale::from_headers(&headers),
        )
        .await?,
    ))
}

async fn stock_snapshot_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<StockSnapshotRequest>,
) -> AppResult<Json<ResearchRecord>> {
    Ok(Json(
        analyze_stock_snapshot(
            &state.pool,
            state.ai.clone(),
            state.market_data.clone(),
            request,
            Locale::from_headers(&headers),
        )
        .await?,
    ))
}

async fn portfolio_review_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> AppResult<Json<ResearchRecord>> {
    Ok(Json(
        review_portfolio(
            &state.pool,
            state.ai.clone(),
            Locale::from_headers(&headers),
        )
        .await?,
    ))
}

async fn adopt_candidates_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<AdoptResearchCandidatesRequest>,
) -> AppResult<Json<InvestmentSystem>> {
    Ok(Json(
        adopt_candidates(&state.pool, &id, request, Locale::from_headers(&headers)).await?,
    ))
}

pub async fn distill(
    pool: &SqlitePool,
    ai: Arc<AiRuntime>,
    request: DistillResearchRequest,
    locale: Locale,
) -> AppResult<ResearchRecord> {
    if request.title.trim().is_empty() {
        return Err(AppError::bad_request("title is required"));
    }
    if request.source_content.trim().is_empty() {
        return Err(AppError::bad_request("source content is required"));
    }

    let input = ResearchSourceInput {
        title: request.title.trim().to_string(),
        source_type: clean_option(request.source_type.clone()),
        source_title: clean_option(request.source_title.clone()),
        source_author: clean_option(request.source_author.clone()),
        source_content: request.source_content.trim().to_string(),
        symbol: clean_option(request.symbol.clone()),
    };
    let analysis = ai
        .distill_research_source(&input, locale)
        .await
        .map_err(|err| AppError::internal(err.to_string()))?;

    create_record(
        pool,
        CreateResearchRecord {
            kind: ResearchRecordKind::Distillation,
            title: input.title,
            source_type: input.source_type,
            source_title: input.source_title,
            source_author: input.source_author,
            source_content: Some(input.source_content),
            symbol: input.symbol,
            memo_id: None,
            analysis,
        },
    )
    .await
}

pub async fn analyze_stock_snapshot(
    pool: &SqlitePool,
    ai: Arc<AiRuntime>,
    market_data: Arc<dyn MarketDataProvider>,
    request: StockSnapshotRequest,
    locale: Locale,
) -> AppResult<ResearchRecord> {
    let symbol = request.symbol.trim().to_ascii_uppercase();
    if symbol.is_empty() {
        return Err(AppError::bad_request("symbol is required"));
    }

    let positions = portfolio::list_positions(pool).await?;
    let position = positions
        .iter()
        .find(|position| position.symbol.eq_ignore_ascii_case(&symbol))
        .cloned();
    let portfolio_summary = portfolio::summary(pool).await?;
    let related_memos = memo::list(pool)
        .await?
        .into_iter()
        .filter(|memo| {
            memo.symbol
                .as_deref()
                .is_some_and(|item| item.eq_ignore_ascii_case(&symbol))
        })
        .collect::<Vec<_>>();
    let selected_memo = match request
        .memo_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        Some(id) => Some(memo::get(pool, id).await?),
        None => None,
    };
    if let Some(memo) = &selected_memo {
        if !memo
            .symbol
            .as_deref()
            .is_some_and(|memo_symbol| memo_symbol.eq_ignore_ascii_case(&symbol))
        {
            return Err(AppError::bad_request("selected memo does not match symbol"));
        }
    }
    let (quote, quote_error) = match market_data.quote(&symbol).await {
        Ok(quote) => (Some(quote), None),
        Err(error) => (None, Some(error.to_string())),
    };

    let context = StockSnapshotContext {
        symbol: symbol.clone(),
        position,
        portfolio_summary,
        related_memos,
        selected_memo: selected_memo.clone(),
        quote,
        quote_error,
    };
    let analysis = ai
        .analyze_stock_snapshot(&context, locale)
        .await
        .map_err(|err| AppError::internal(err.to_string()))?;

    create_record(
        pool,
        CreateResearchRecord {
            kind: ResearchRecordKind::StockSnapshot,
            title: format!("{symbol} stock snapshot"),
            source_type: Some("portfolio".to_string()),
            source_title: Some(format!("{symbol} current context")),
            source_author: None,
            source_content: Some(serde_json::to_string_pretty(&context)?),
            symbol: Some(symbol),
            memo_id: selected_memo.map(|memo| memo.id),
            analysis,
        },
    )
    .await
}

pub async fn review_portfolio(
    pool: &SqlitePool,
    ai: Arc<AiRuntime>,
    locale: Locale,
) -> AppResult<ResearchRecord> {
    let positions = portfolio::list_positions(pool).await?;
    if positions.is_empty() {
        return Err(AppError::bad_request("portfolio has no positions"));
    }

    let summary = portfolio::summary(pool).await?;
    let memos = memo::list(pool).await?;
    let holdings_without_memo = positions
        .iter()
        .filter(|position| {
            !memos.iter().any(|memo| {
                memo.symbol
                    .as_deref()
                    .is_some_and(|symbol| symbol.eq_ignore_ascii_case(&position.symbol))
                    && !memo.thesis.trim().is_empty()
            })
        })
        .map(|position| position.symbol.clone())
        .collect::<Vec<_>>();

    let context = PortfolioReviewContext {
        positions,
        summary,
        holdings_without_memo,
    };
    let analysis = ai
        .review_portfolio_risk(&context, locale)
        .await
        .map_err(|err| AppError::internal(err.to_string()))?;

    create_record(
        pool,
        CreateResearchRecord {
            kind: ResearchRecordKind::PortfolioReview,
            title: "Portfolio risk review".to_string(),
            source_type: Some("portfolio".to_string()),
            source_title: Some("Current portfolio".to_string()),
            source_author: None,
            source_content: Some(serde_json::to_string_pretty(&context)?),
            symbol: None,
            memo_id: None,
            analysis,
        },
    )
    .await
}

pub async fn adopt_candidates(
    pool: &SqlitePool,
    record_id: &str,
    request: AdoptResearchCandidatesRequest,
    locale: Locale,
) -> AppResult<InvestmentSystem> {
    let record = get_record(pool, record_id).await?;
    let selected_principles = matching_candidates(request.principles, &record.candidate_principles);
    let selected_checklist =
        matching_candidates(request.checklist_items, &record.candidate_checklist_items);

    if selected_principles.is_empty() && selected_checklist.is_empty() {
        return Err(AppError::bad_request(
            "no selected candidates match this record",
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

pub async fn create_record(
    pool: &SqlitePool,
    request: CreateResearchRecord,
) -> AppResult<ResearchRecord> {
    if request.title.trim().is_empty() {
        return Err(AppError::bad_request("title is required"));
    }
    if request.analysis.summary.trim().is_empty() {
        return Err(AppError::bad_request("analysis summary is required"));
    }

    let now = now_iso();
    let raw_output = serde_json::to_value(&request.analysis)?;
    let record = ResearchRecord {
        id: Uuid::new_v4().to_string(),
        kind: request.kind,
        title: request.title.trim().to_string(),
        source_type: request.source_type.and_then(non_empty_string),
        source_title: request.source_title.and_then(non_empty_string),
        source_author: request.source_author.and_then(non_empty_string),
        source_content: request.source_content.and_then(non_empty_string),
        symbol: request.symbol.and_then(normalize_symbol),
        memo_id: request.memo_id.and_then(non_empty_string),
        summary: request.analysis.summary.trim().to_string(),
        insights: request.analysis.insights,
        risks: request.analysis.risks,
        checklist: request.analysis.checklist,
        candidate_principles: request.analysis.candidate_principles,
        candidate_checklist_items: request.analysis.candidate_checklist_items,
        raw_output,
        created_at: now.clone(),
        updated_at: now,
    };

    sqlx::query(
        r#"
        INSERT INTO research_records (
            id, kind, title, source_type, source_title, source_author, source_content,
            symbol, memo_id, summary, insights_json, risks_json, checklist_json,
            candidate_principles_json, candidate_checklist_items_json, raw_output_json,
            created_at, updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&record.id)
    .bind(record.kind.as_str())
    .bind(&record.title)
    .bind(&record.source_type)
    .bind(&record.source_title)
    .bind(&record.source_author)
    .bind(&record.source_content)
    .bind(&record.symbol)
    .bind(&record.memo_id)
    .bind(&record.summary)
    .bind(serde_json::to_string(&record.insights)?)
    .bind(serde_json::to_string(&record.risks)?)
    .bind(serde_json::to_string(&record.checklist)?)
    .bind(serde_json::to_string(&record.candidate_principles)?)
    .bind(serde_json::to_string(&record.candidate_checklist_items)?)
    .bind(serde_json::to_string(&record.raw_output)?)
    .bind(&record.created_at)
    .bind(&record.updated_at)
    .execute(pool)
    .await?;

    Ok(record)
}

pub async fn list_records(
    pool: &SqlitePool,
    query: ResearchRecordQuery,
) -> AppResult<Vec<ResearchRecord>> {
    let mut builder: QueryBuilder<'_, Sqlite> = QueryBuilder::new(
        r#"
        SELECT id, kind, title, source_type, source_title, source_author, source_content,
               symbol, memo_id, summary, insights_json, risks_json, checklist_json,
               candidate_principles_json, candidate_checklist_items_json, raw_output_json,
               created_at, updated_at
        FROM research_records
        "#,
    );

    let mut has_filter = false;
    if let Some(kind) = query.kind {
        push_filter(&mut builder, &mut has_filter);
        builder.push("kind = ").push_bind(kind.as_str());
    }
    if let Some(symbol) = query.symbol.and_then(normalize_symbol) {
        push_filter(&mut builder, &mut has_filter);
        builder.push("UPPER(symbol) = ").push_bind(symbol);
    }
    if let Some(q) = query.q.and_then(non_empty_string) {
        let pattern = format!("%{}%", q.to_lowercase());
        push_filter(&mut builder, &mut has_filter);
        builder
            .push("(LOWER(title) LIKE ")
            .push_bind(pattern.clone())
            .push(" OR LOWER(summary) LIKE ")
            .push_bind(pattern.clone())
            .push(" OR LOWER(source_title) LIKE ")
            .push_bind(pattern.clone())
            .push(" OR LOWER(source_author) LIKE ")
            .push_bind(pattern)
            .push(")");
    }
    builder.push(" ORDER BY updated_at DESC");

    let rows = builder.build().fetch_all(pool).await?;
    rows.into_iter().map(record_from_row).collect()
}

pub async fn get_record(pool: &SqlitePool, id: &str) -> AppResult<ResearchRecord> {
    let row = sqlx::query(
        r#"
        SELECT id, kind, title, source_type, source_title, source_author, source_content,
               symbol, memo_id, summary, insights_json, risks_json, checklist_json,
               candidate_principles_json, candidate_checklist_items_json, raw_output_json,
               created_at, updated_at
        FROM research_records
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::not_found("research record not found"))?;

    record_from_row(row)
}

fn record_from_row(row: sqlx::sqlite::SqliteRow) -> AppResult<ResearchRecord> {
    let kind: String = row.try_get("kind")?;
    let raw_output_json: String = row.try_get("raw_output_json")?;
    Ok(ResearchRecord {
        id: row.try_get("id")?,
        kind: ResearchRecordKind::parse(&kind)
            .ok_or_else(|| AppError::internal("invalid research record kind"))?,
        title: row.try_get("title")?,
        source_type: row.try_get("source_type")?,
        source_title: row.try_get("source_title")?,
        source_author: row.try_get("source_author")?,
        source_content: row.try_get("source_content")?,
        symbol: row.try_get("symbol")?,
        memo_id: row.try_get("memo_id")?,
        summary: row.try_get("summary")?,
        insights: parse_json_array(row.try_get("insights_json")?),
        risks: parse_json_array(row.try_get("risks_json")?),
        checklist: parse_json_array(row.try_get("checklist_json")?),
        candidate_principles: parse_json_array(row.try_get("candidate_principles_json")?),
        candidate_checklist_items: parse_json_array(row.try_get("candidate_checklist_items_json")?),
        raw_output: serde_json::from_str(&raw_output_json)?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn parse_json_array(value: String) -> Vec<String> {
    serde_json::from_str(&value).unwrap_or_default()
}

fn push_filter(builder: &mut QueryBuilder<'_, Sqlite>, has_filter: &mut bool) {
    if *has_filter {
        builder.push(" AND ");
    } else {
        builder.push(" WHERE ");
        *has_filter = true;
    }
}

fn non_empty_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn normalize_symbol(value: String) -> Option<String> {
    non_empty_string(value).map(|value| value.to_uppercase())
}

fn clean_option(value: Option<String>) -> Option<String> {
    value.and_then(non_empty_string)
}

fn matching_candidates(selected: Vec<String>, allowed: &[String]) -> Vec<String> {
    selected
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .filter(|value| allowed.iter().any(|allowed| allowed.trim() == value))
        .collect()
}

fn dedupe(values: Vec<String>) -> Vec<String> {
    let mut result = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if !trimmed.is_empty() && !result.iter().any(|existing: &String| existing == trimmed) {
            result.push(trimmed.to_string());
        }
    }
    result
}
