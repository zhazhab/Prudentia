use axum::Router;
use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
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

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ResearchRecordQuery {
    pub kind: Option<ResearchRecordKind>,
    pub symbol: Option<String>,
    pub q: Option<String>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
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
