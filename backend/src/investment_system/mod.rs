use axum::{
    extract::State,
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::{
    ai::InvestmentSystemRefinement,
    error::{AppError, AppResult},
    locale::Locale,
    state::AppState,
    time::now_iso,
};

const DEFAULT_SYSTEM_ID: &str = "default";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvestmentSystem {
    pub principles: Vec<String>,
    pub checklist_items: Vec<String>,
    pub circle_of_competence: Vec<String>,
    pub decision_rules: Vec<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateInvestmentSystemRequest {
    pub principles: Option<Vec<String>>,
    pub checklist_items: Option<Vec<String>>,
    pub circle_of_competence: Option<Vec<String>>,
    pub decision_rules: Option<Vec<String>>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(get_system).patch(update_system))
        .route("/ai/refine", post(refine_system))
}

async fn get_system(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> AppResult<Json<InvestmentSystem>> {
    Ok(Json(
        get_or_default_with_locale(&state.pool, Locale::from_headers(&headers)).await?,
    ))
}

async fn update_system(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<UpdateInvestmentSystemRequest>,
) -> AppResult<Json<InvestmentSystem>> {
    Ok(Json(
        update_with_locale(&state.pool, request, Locale::from_headers(&headers)).await?,
    ))
}

async fn refine_system(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> AppResult<Json<InvestmentSystemRefinement>> {
    let locale = Locale::from_headers(&headers);
    let system = get_or_default_with_locale(&state.pool, locale).await?;
    let refinement = state
        .ai
        .refine_system(&system, locale)
        .await
        .map_err(|err| AppError::internal(err.to_string()))?;
    Ok(Json(refinement))
}

pub async fn get_or_default(pool: &SqlitePool) -> AppResult<InvestmentSystem> {
    get_or_default_with_locale(pool, Locale::En).await
}

pub async fn get_or_default_with_locale(
    pool: &SqlitePool,
    locale: Locale,
) -> AppResult<InvestmentSystem> {
    let row = sqlx::query(
        r#"
        SELECT principles_json, checklist_items_json, circle_of_competence_json,
               decision_rules_json, updated_at
        FROM investment_system
        WHERE id = ?
        "#,
    )
    .bind(DEFAULT_SYSTEM_ID)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(row) => system_from_row(row),
        None => Ok(default_system(locale)),
    }
}

pub async fn update(
    pool: &SqlitePool,
    request: UpdateInvestmentSystemRequest,
) -> AppResult<InvestmentSystem> {
    update_with_locale(pool, request, Locale::En).await
}

pub async fn update_with_locale(
    pool: &SqlitePool,
    request: UpdateInvestmentSystemRequest,
    locale: Locale,
) -> AppResult<InvestmentSystem> {
    let mut system = get_or_default_with_locale(pool, locale).await?;

    if let Some(principles) = request.principles {
        system.principles = clean_list(principles);
    }
    if let Some(checklist_items) = request.checklist_items {
        system.checklist_items = clean_list(checklist_items);
    }
    if let Some(circle_of_competence) = request.circle_of_competence {
        system.circle_of_competence = clean_list(circle_of_competence);
    }
    if let Some(decision_rules) = request.decision_rules {
        system.decision_rules = clean_list(decision_rules);
    }
    system.updated_at = now_iso();

    sqlx::query(
        r#"
        INSERT INTO investment_system (
            id, principles_json, checklist_items_json, circle_of_competence_json,
            decision_rules_json, updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            principles_json = excluded.principles_json,
            checklist_items_json = excluded.checklist_items_json,
            circle_of_competence_json = excluded.circle_of_competence_json,
            decision_rules_json = excluded.decision_rules_json,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(DEFAULT_SYSTEM_ID)
    .bind(serde_json::to_string(&system.principles)?)
    .bind(serde_json::to_string(&system.checklist_items)?)
    .bind(serde_json::to_string(&system.circle_of_competence)?)
    .bind(serde_json::to_string(&system.decision_rules)?)
    .bind(&system.updated_at)
    .execute(pool)
    .await?;

    Ok(system)
}

fn system_from_row(row: sqlx::sqlite::SqliteRow) -> AppResult<InvestmentSystem> {
    Ok(InvestmentSystem {
        principles: serde_json::from_str(&row.try_get::<String, _>("principles_json")?)
            .unwrap_or_default(),
        checklist_items: serde_json::from_str(&row.try_get::<String, _>("checklist_items_json")?)
            .unwrap_or_default(),
        circle_of_competence: serde_json::from_str(
            &row.try_get::<String, _>("circle_of_competence_json")?,
        )
        .unwrap_or_default(),
        decision_rules: serde_json::from_str(&row.try_get::<String, _>("decision_rules_json")?)
            .unwrap_or_default(),
        updated_at: row.try_get("updated_at")?,
    })
}

fn default_system(locale: Locale) -> InvestmentSystem {
    if locale.is_zh() {
        InvestmentSystem {
            principles: vec![
                "先写 thesis，再看价格走势".to_string(),
                "反证证据必须优先落笔".to_string(),
            ],
            checklist_items: vec![
                "这笔投资成立必须满足什么条件？".to_string(),
                "什么情况会证明我错了？".to_string(),
                "仓位是否匹配不确定性？".to_string(),
            ],
            circle_of_competence: vec![],
            decision_rules: vec![
                "每个新持仓都必须有复盘日期".to_string(),
                "没有更新备忘录，不加仓".to_string(),
            ],
            updated_at: now_iso(),
        }
    } else {
        InvestmentSystem {
            principles: vec![
                "Thesis before price action".to_string(),
                "Disconfirming evidence gets written down first".to_string(),
            ],
            checklist_items: vec![
                "What has to be true for this to work?".to_string(),
                "What would prove me wrong?".to_string(),
                "Is position size consistent with uncertainty?".to_string(),
            ],
            circle_of_competence: vec![],
            decision_rules: vec![
                "Every new position needs a review date".to_string(),
                "No size increase without an updated memo".to_string(),
            ],
            updated_at: now_iso(),
        }
    }
}

fn clean_list(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}
