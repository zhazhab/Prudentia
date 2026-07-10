use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};

use crate::{
    ai::ConversationResearchSource,
    config::AppConfig,
    error::{AppError, AppResult},
    time::now_iso,
};

#[async_trait]
pub trait WebResearchProvider: Send + Sync {
    fn enabled(&self) -> bool;
    async fn search(&self, query: &str) -> Result<Vec<ConversationResearchSource>, String>;
}

pub fn provider_from_config(config: &AppConfig) -> Arc<dyn WebResearchProvider> {
    match config
        .web_research_provider
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "tavily" if config.tavily_api_key.is_some() => Arc::new(TavilyResearchProvider {
            client: Client::new(),
            api_key: config.tavily_api_key.clone().expect("checked Tavily key"),
        }),
        "tavily" => {
            tracing::warn!("WEB_RESEARCH_PROVIDER=tavily was set without TAVILY_API_KEY");
            Arc::new(DisabledResearchProvider)
        }
        _ => Arc::new(DisabledResearchProvider),
    }
}

pub async fn search_with_cache(
    pool: &SqlitePool,
    provider: Arc<dyn WebResearchProvider>,
    query: &str,
) -> AppResult<Vec<ConversationResearchSource>> {
    let hash = query_hash(query);
    if let Some(cached) = load_cache(pool, &hash).await? {
        return Ok(cached);
    }
    if !provider.enabled() {
        return Err(AppError::bad_request(
            "external research provider is not configured",
        ));
    }
    let results = provider
        .search(query)
        .await
        .map_err(|error| AppError::bad_request(format!("external research failed: {error}")))?;
    sqlx::query(
        r#"INSERT OR REPLACE INTO conversation_research_cache (
            query_hash, query, results_json, fetched_at
        ) VALUES (?, ?, ?, ?)"#,
    )
    .bind(hash)
    .bind(query)
    .bind(serde_json::to_string(&results)?)
    .bind(now_iso())
    .execute(pool)
    .await?;
    Ok(results)
}

pub fn should_research(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    [
        "最新",
        "今天",
        "近期",
        "搜索",
        "查一下",
        "查证",
        "新闻",
        "财报",
        "公告",
        "估值",
        "latest",
        "today",
        "current",
        "search",
        "verify",
        "news",
        "earnings",
        "filing",
    ]
    .iter()
    .any(|keyword| normalized.contains(keyword))
}

struct DisabledResearchProvider;

#[async_trait]
impl WebResearchProvider for DisabledResearchProvider {
    fn enabled(&self) -> bool {
        false
    }

    async fn search(&self, _query: &str) -> Result<Vec<ConversationResearchSource>, String> {
        Err("external research provider is disabled".to_string())
    }
}

struct TavilyResearchProvider {
    client: Client,
    api_key: String,
}

#[async_trait]
impl WebResearchProvider for TavilyResearchProvider {
    fn enabled(&self) -> bool {
        true
    }

    async fn search(&self, query: &str) -> Result<Vec<ConversationResearchSource>, String> {
        let response: TavilyResponse = self
            .client
            .post("https://api.tavily.com/search")
            .timeout(std::time::Duration::from_secs(20))
            .json(&TavilyRequest {
                api_key: &self.api_key,
                query,
                search_depth: "advanced",
                max_results: 5,
            })
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?
            .json()
            .await
            .map_err(|error| error.to_string())?;
        Ok(response
            .results
            .into_iter()
            .map(|result| ConversationResearchSource {
                source_tier: source_tier(&result.url).to_string(),
                title: result.title,
                url: result.url,
                snippet: result.content,
            })
            .collect())
    }
}

#[derive(Serialize)]
struct TavilyRequest<'a> {
    api_key: &'a str,
    query: &'a str,
    search_depth: &'a str,
    max_results: usize,
}

#[derive(Deserialize)]
struct TavilyResponse {
    #[serde(default)]
    results: Vec<TavilyResult>,
}

#[derive(Deserialize)]
struct TavilyResult {
    title: String,
    url: String,
    content: String,
}

async fn load_cache(
    pool: &SqlitePool,
    hash: &str,
) -> AppResult<Option<Vec<ConversationResearchSource>>> {
    let row = sqlx::query(
        "SELECT results_json, fetched_at FROM conversation_research_cache WHERE query_hash = ?",
    )
    .bind(hash)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let fetched_at: String = row.try_get("fetched_at")?;
    let fresh = chrono::DateTime::parse_from_rfc3339(&fetched_at)
        .map(|fetched| {
            chrono::Utc::now() - fetched.with_timezone(&chrono::Utc) < chrono::Duration::hours(24)
        })
        .unwrap_or(false);
    if !fresh {
        return Ok(None);
    }
    Ok(Some(serde_json::from_str(
        &row.try_get::<String, _>("results_json")?,
    )?))
}

fn query_hash(query: &str) -> String {
    Sha256::digest(query.trim().as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn source_tier(url: &str) -> &'static str {
    let lower = url.to_ascii_lowercase();
    if lower.contains("sec.gov")
        || lower.contains("hkexnews.hk")
        || lower.contains("sse.com.cn")
        || lower.contains("szse.cn")
        || lower.contains("investor")
    {
        "primary"
    } else {
        "secondary"
    }
}
