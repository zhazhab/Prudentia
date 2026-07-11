use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};

use crate::{
    ai::ConversationResearchSource,
    config::{AppConfig, WebResearchProviderKind},
    error::{AppError, AppResult},
    time::now_iso,
};

use super::types::ThreadSubject;

mod public_sources;
mod source_validation;
use source_validation::{normalize_company_sources, normalize_sources, verify_source_urls};

const RESEARCH_CACHE_TTL: chrono::Duration = chrono::Duration::hours(24);

#[async_trait]
pub trait WebResearchProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn enabled(&self) -> bool;
    async fn search(
        &self,
        request: &CompanyResearchRequest,
    ) -> Result<ResearchOutcome, ResearchError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ResearchError {
    #[error("research request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("research payload could not be parsed: {0}")]
    Payload(String),
    #[error("external research is explicitly disabled")]
    Disabled,
    #[error("external research configuration is invalid: {0}")]
    Configuration(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResearchOutcome {
    pub sources: Vec<ConversationResearchSource>,
    pub warning: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EvidenceCategory {
    Official,
    Independent,
    Community,
}

impl EvidenceCategory {
    fn label(self) -> &'static str {
        match self {
            Self::Official => "official filings",
            Self::Independent => "independent analysis",
            Self::Community => "community viewpoints",
        }
    }

    fn accepts(self, source: &ConversationResearchSource) -> bool {
        match self {
            Self::Official => source.source_tier == "primary",
            Self::Independent => source.source_tier == "secondary",
            Self::Community => source.source_tier == "community",
        }
    }

    fn source_tier(self) -> &'static str {
        match self {
            Self::Official => "primary",
            Self::Independent => "secondary",
            Self::Community => "community",
        }
    }
}

struct PlannedResearchQuery {
    category: EvidenceCategory,
    text: String,
}

fn incomplete_research_warning(categories: &[EvidenceCategory]) -> Option<String> {
    (!categories.is_empty()).then(|| {
        format!(
            "External verification was incomplete for: {}.",
            categories
                .iter()
                .map(|category| category.label())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })
}

pub fn provider_from_config(config: &AppConfig) -> Arc<dyn WebResearchProvider> {
    match &config.web_research_provider {
        WebResearchProviderKind::PublicSources => Arc::new(PublicSourcesResearchProvider {
            client: Client::new(),
        }),
        WebResearchProviderKind::Tavily if config.tavily_api_key.is_some() => {
            Arc::new(TavilyResearchProvider {
                client: Client::new(),
                api_key: config.tavily_api_key.clone().expect("checked Tavily key"),
            })
        }
        WebResearchProviderKind::Tavily => {
            tracing::warn!("WEB_RESEARCH_PROVIDER=tavily was set without TAVILY_API_KEY");
            Arc::new(MisconfiguredResearchProvider {
                reason: "Tavily requires TAVILY_API_KEY".to_string(),
            })
        }
        WebResearchProviderKind::Disabled => Arc::new(DisabledResearchProvider),
        WebResearchProviderKind::Invalid(value) => Arc::new(MisconfiguredResearchProvider {
            reason: format!("unsupported WEB_RESEARCH_PROVIDER '{value}'"),
        }),
    }
}

pub async fn search_with_cache(
    pool: &SqlitePool,
    provider: Arc<dyn WebResearchProvider>,
    request: &CompanyResearchRequest,
) -> AppResult<ResearchOutcome> {
    let cache_key = request.cache_key();
    let hash = query_hash(&format!("{}:{cache_key}", provider.name()));
    if let Some(cached) = load_cache(pool, &hash)
        .await?
        .filter(|outcome| !outcome.sources.is_empty())
    {
        return Ok(cached);
    }
    if !provider.enabled() {
        return Err(AppError::bad_request(
            "external research provider is not configured",
        ));
    }
    let outcome = provider
        .search(request)
        .await
        .map_err(|error| AppError::bad_request(format!("external research failed: {error}")))?;
    let outcome = require_usable_sources(outcome)?;
    sqlx::query(
        r#"INSERT OR REPLACE INTO conversation_research_cache (
            query_hash, query, results_json, fetched_at
        ) VALUES (?, ?, ?, ?)"#,
    )
    .bind(hash)
    .bind(cache_key)
    .bind(serde_json::to_string(&outcome)?)
    .bind(now_iso())
    .execute(pool)
    .await?;
    Ok(outcome)
}

fn require_usable_sources(outcome: ResearchOutcome) -> AppResult<ResearchOutcome> {
    if outcome.sources.is_empty() {
        Err(AppError::bad_request(
            "external research returned no usable sources",
        ))
    } else {
        Ok(outcome)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CompanyResearchRequest {
    company_name: String,
    symbol: String,
    intent: CompanyResearchIntent,
}

impl CompanyResearchRequest {
    fn cache_key(&self) -> String {
        serde_json::to_string(self).expect("company research request is serializable")
    }

    fn subject_terms(&self) -> String {
        if self.company_name.eq_ignore_ascii_case(&self.symbol) || self.symbol.is_empty() {
            self.company_name.clone()
        } else {
            format!("{} {}", self.company_name, self.symbol)
        }
    }

    fn base_symbol(&self) -> String {
        self.symbol
            .split('.')
            .next()
            .unwrap_or(self.symbol.as_str())
            .to_ascii_uppercase()
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum CompanyResearchIntent {
    Earnings,
    News,
    Valuation,
    Risk,
    Fundamentals,
    General,
}

impl CompanyResearchIntent {
    fn query_terms(self) -> &'static str {
        match self {
            Self::Earnings => "latest earnings financial results",
            Self::News => "latest company news announcement",
            Self::Valuation => "current valuation expectations",
            Self::Risk => "current business risks competition regulation",
            Self::Fundamentals => "current fundamentals growth margins cash flow",
            Self::General => "current company analysis",
        }
    }
}

pub fn plan_research(message: &str, subject: &ThreadSubject) -> Option<CompanyResearchRequest> {
    if subject.kind != "company" || super::is_simple_social_turn(message) {
        return None;
    }
    let normalized = message.to_ascii_lowercase();
    let intent = if contains_any(&normalized, &["财报", "业绩", "earnings", "filing"]) {
        CompanyResearchIntent::Earnings
    } else if contains_any(&normalized, &["新闻", "公告", "news", "announcement"]) {
        CompanyResearchIntent::News
    } else if contains_any(&normalized, &["估值", "valuation"]) {
        CompanyResearchIntent::Valuation
    } else if contains_any(
        &normalized,
        &["风险", "竞争", "risk", "competition", "regulation"],
    ) {
        CompanyResearchIntent::Risk
    } else if contains_any(
        &normalized,
        &[
            "基本面",
            "增长",
            "利润",
            "收入",
            "fundamentals",
            "growth",
            "margin",
            "revenue",
        ],
    ) {
        CompanyResearchIntent::Fundamentals
    } else {
        CompanyResearchIntent::General
    };
    let symbol = subject.subject_key.as_deref().unwrap_or_default();
    let label = subject.label.as_deref().unwrap_or(symbol);
    Some(CompanyResearchRequest {
        company_name: label.to_string(),
        symbol: symbol.to_string(),
        intent,
    })
}

fn contains_any(value: &str, candidates: &[&str]) -> bool {
    candidates.iter().any(|candidate| value.contains(candidate))
}

struct DisabledResearchProvider;

#[async_trait]
impl WebResearchProvider for DisabledResearchProvider {
    fn name(&self) -> &'static str {
        "disabled"
    }

    fn enabled(&self) -> bool {
        false
    }

    async fn search(
        &self,
        _request: &CompanyResearchRequest,
    ) -> Result<ResearchOutcome, ResearchError> {
        Err(ResearchError::Disabled)
    }
}

struct MisconfiguredResearchProvider {
    reason: String,
}

#[async_trait]
impl WebResearchProvider for MisconfiguredResearchProvider {
    fn name(&self) -> &'static str {
        "misconfigured"
    }

    fn enabled(&self) -> bool {
        true
    }

    async fn search(
        &self,
        _request: &CompanyResearchRequest,
    ) -> Result<ResearchOutcome, ResearchError> {
        Err(ResearchError::Configuration(self.reason.clone()))
    }
}

struct PublicSourcesResearchProvider {
    client: Client,
}

#[async_trait]
impl WebResearchProvider for PublicSourcesResearchProvider {
    fn name(&self) -> &'static str {
        "public_sources"
    }

    fn enabled(&self) -> bool {
        true
    }

    async fn search(
        &self,
        request: &CompanyResearchRequest,
    ) -> Result<ResearchOutcome, ResearchError> {
        let (official, independent, community) = tokio::join!(
            public_sources::official_filings(&self.client, request),
            public_sources::independent_news(&self.client, request),
            public_sources::community_discussions(&self.client, request),
        );
        let mut failed_categories = Vec::new();
        let official = official.unwrap_or_else(|error| {
            tracing::warn!(category = "official filings", %error, "research source failed");
            failed_categories.push(EvidenceCategory::Official);
            Vec::new()
        });
        let independent = independent.unwrap_or_else(|error| {
            tracing::warn!(category = "independent analysis", %error, "research source failed");
            failed_categories.push(EvidenceCategory::Independent);
            Vec::new()
        });
        let community = community.unwrap_or_else(|error| {
            tracing::warn!(category = "community viewpoints", %error, "research source failed");
            failed_categories.push(EvidenceCategory::Community);
            Vec::new()
        });

        let official = verify_source_urls(normalize_sources(official)).await;
        let independent = verify_source_urls(normalize_sources(independent)).await;
        let community = verify_source_urls(normalize_sources(community)).await;
        for (category, is_empty) in [
            (EvidenceCategory::Official, official.is_empty()),
            (EvidenceCategory::Independent, independent.is_empty()),
            (EvidenceCategory::Community, community.is_empty()),
        ] {
            if is_empty && !failed_categories.contains(&category) {
                failed_categories.push(category);
            }
        }
        let mut sources = official;
        sources.extend(independent);
        sources.extend(community);
        let sources = normalize_sources(sources);
        let warning = incomplete_research_warning(&failed_categories);
        Ok(ResearchOutcome { sources, warning })
    }
}

struct TavilyResearchProvider {
    client: Client,
    api_key: String,
}

#[async_trait]
impl WebResearchProvider for TavilyResearchProvider {
    fn name(&self) -> &'static str {
        "tavily"
    }

    fn enabled(&self) -> bool {
        true
    }

    async fn search(
        &self,
        request: &CompanyResearchRequest,
    ) -> Result<ResearchOutcome, ResearchError> {
        let mut sources = Vec::new();
        let mut failed_categories = Vec::new();
        for planned_query in research_queries(request) {
            let results = match self.search_query(&planned_query.text).await {
                Ok(results) => results,
                Err(error) => {
                    tracing::warn!(category = planned_query.category.label(), %error, "Tavily research query failed");
                    failed_categories.push(planned_query.category);
                    continue;
                }
            };
            let candidates = results
                .into_iter()
                .map(|result| ConversationResearchSource {
                    source_tier: planned_query.category.source_tier().to_string(),
                    title: result.title,
                    url: result.url,
                    snippet: result.content,
                })
                .collect();
            let verified = verify_source_urls(normalize_company_sources(
                candidates,
                &request.company_name,
                &request.symbol,
            ))
            .await
            .into_iter()
            .filter(|source| planned_query.category.accepts(source))
            .collect::<Vec<_>>();
            if verified.is_empty() && !failed_categories.contains(&planned_query.category) {
                failed_categories.push(planned_query.category);
            }
            sources.extend(verified);
        }
        Ok(ResearchOutcome {
            sources: normalize_sources(sources),
            warning: incomplete_research_warning(&failed_categories),
        })
    }
}

impl TavilyResearchProvider {
    async fn search_query(&self, query: &str) -> Result<Vec<TavilyResult>, ResearchError> {
        let response: TavilyResponse = self
            .client
            .post("https://api.tavily.com/search")
            .timeout(std::time::Duration::from_secs(20))
            .json(&TavilyRequest {
                api_key: &self.api_key,
                query,
                search_depth: "advanced",
                max_results: 4,
            })
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(response.results)
    }
}

fn research_queries(request: &CompanyResearchRequest) -> [PlannedResearchQuery; 3] {
    let subject = request.subject_terms();
    let intent = request.intent.query_terms();
    [
        PlannedResearchQuery {
            category: EvidenceCategory::Official,
            text: format!(
                "{subject} {intent} official investor relations latest earnings filing announcement"
            ),
        },
        PlannedResearchQuery {
            category: EvidenceCategory::Independent,
            text: format!("{subject} {intent} independent analysis risks competition valuation"),
        },
        PlannedResearchQuery {
            category: EvidenceCategory::Community,
            text: format!(
                "{subject} {intent} popular investor discussion comments site:xueqiu.com OR site:reddit.com OR site:stocktwits.com OR site:moomoo.com OR site:guba.eastmoney.com"
            ),
        },
    ]
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

async fn load_cache(pool: &SqlitePool, hash: &str) -> AppResult<Option<ResearchOutcome>> {
    let row = sqlx::query(
        "SELECT results_json, fetched_at FROM conversation_research_cache WHERE query_hash = ?",
    )
    .bind(hash)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let raw: String = row.try_get("results_json")?;
    let outcome = serde_json::from_str::<ResearchOutcome>(&raw)?;
    let fetched_at: String = row.try_get("fetched_at")?;
    let fresh = chrono::DateTime::parse_from_rfc3339(&fetched_at)
        .map(|fetched| {
            chrono::Utc::now() - fetched.with_timezone(&chrono::Utc) < RESEARCH_CACHE_TTL
        })
        .unwrap_or(false);
    if !fresh {
        return Ok(None);
    }
    Ok(Some(outcome))
}

fn query_hash(query: &str) -> String {
    Sha256::digest(query.trim().as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{
        incomplete_research_warning, plan_research, require_usable_sources, research_queries,
        CompanyResearchRequest, DisabledResearchProvider, EvidenceCategory,
        MisconfiguredResearchProvider, PublicSourcesResearchProvider, ResearchError,
        WebResearchProvider, RESEARCH_CACHE_TTL,
    };
    use crate::conversation::types::ThreadSubject;

    #[test]
    fn company_analysis_requests_trigger_external_research() {
        let company = ThreadSubject {
            kind: "company".to_string(),
            subject_key: Some("PDD".to_string()),
            label: Some("PDD Holdings".to_string()),
            confidence: 0.95,
        };
        assert!(plan_research("分析一下 PDD", &company).is_some());
        assert!(plan_research("What do you think about PDD's margins?", &company).is_some());
        assert!(plan_research("PDD 值得买吗？", &company).is_some());
        assert!(plan_research("Should I buy PDD?", &company).is_some());
        assert!(plan_research("PDD 的护城河是什么？", &company).is_some());
        assert!(plan_research("你好", &company).is_none());
        assert!(plan_research("分析一下我的持仓", &ThreadSubject::default()).is_none());
        let request = plan_research("我持有 587 股，请搜索 PDD 最新财报", &company)
            .expect("company research request");
        let queries = research_queries(&request);
        assert_eq!(queries.len(), 3);
        assert!(queries
            .iter()
            .all(|query| query.text.contains("PDD Holdings PDD")));
        assert!(queries
            .iter()
            .all(|query| query.text.contains("latest earnings")));
        assert!(queries.iter().all(|query| !query.text.contains("587")));
    }

    #[test]
    fn research_plans_primary_analysis_and_community_queries() {
        let company = ThreadSubject {
            kind: "company".to_string(),
            subject_key: Some("PDD".to_string()),
            label: Some("PDD Holdings".to_string()),
            confidence: 0.95,
        };
        let request = plan_research("分析 PDD 最新财报", &company).expect("research request");
        let queries = research_queries(&request);

        assert_eq!(queries[0].category, EvidenceCategory::Official);
        assert_eq!(queries[1].category, EvidenceCategory::Independent);
        assert_eq!(queries[2].category, EvidenceCategory::Community);
        assert!(queries[0].text.contains("official investor relations"));
        assert!(queries[1].text.contains("independent analysis"));
        assert!(queries[2].text.contains("xueqiu.com"));
        assert!(queries[2].text.contains("reddit.com"));
    }

    #[test]
    fn partial_research_names_each_incomplete_evidence_category() {
        let warning =
            incomplete_research_warning(&[EvidenceCategory::Official, EvidenceCategory::Community])
                .expect("partial warning");

        assert!(warning.contains("official filings"));
        assert!(warning.contains("community viewpoints"));
        assert!(incomplete_research_warning(&[]).is_none());
    }

    #[test]
    fn complete_and_partial_research_share_the_daily_cache_ttl() {
        assert_eq!(RESEARCH_CACHE_TTL, chrono::Duration::hours(24));
    }

    #[tokio::test]
    async fn only_explicit_disabled_configuration_turns_research_off() {
        let disabled = DisabledResearchProvider;
        assert!(!disabled.enabled());

        let invalid = MisconfiguredResearchProvider {
            reason: "unsupported provider".to_string(),
        };
        assert!(invalid.enabled());
        let request = CompanyResearchRequest {
            company_name: "PDD Holdings".to_string(),
            symbol: "PDD".to_string(),
            intent: super::CompanyResearchIntent::General,
        };
        let error = invalid.search(&request).await.expect_err("invalid config");
        assert!(matches!(error, ResearchError::Configuration(_)));
    }

    #[test]
    fn empty_research_results_are_an_explicit_failure() {
        let error = require_usable_sources(super::ResearchOutcome {
            sources: Vec::new(),
            warning: None,
        })
        .expect_err("empty results must fail");
        assert!(error.to_string().contains("no usable sources"));
    }

    #[tokio::test]
    #[ignore = "requires public internet"]
    async fn public_sources_find_live_pdd_evidence() {
        let company = ThreadSubject {
            kind: "company".to_string(),
            subject_key: Some("PDD".to_string()),
            label: Some("PDD Holdings".to_string()),
            confidence: 0.95,
        };
        let request = plan_research("PDD 最新财报和市场观点", &company).expect("research request");
        let provider: Arc<dyn WebResearchProvider> = Arc::new(PublicSourcesResearchProvider {
            client: reqwest::Client::new(),
        });

        let outcome = provider
            .search(&request)
            .await
            .expect("live public sources");
        for source in &outcome.sources {
            println!("{} | {} | {}", source.source_tier, source.title, source.url);
        }

        assert!(outcome
            .sources
            .iter()
            .any(|source| source.source_tier == "primary" && source.url.contains("sec.gov")));
        assert!(outcome
            .sources
            .iter()
            .filter(|source| source.source_tier == "primary")
            .any(|source| source.snippet.contains("Filing excerpt:")));
        assert!(outcome
            .sources
            .iter()
            .filter(|source| source.source_tier == "primary")
            .any(|source| source.snippet.contains("Total revenues")));
        assert!(outcome
            .sources
            .iter()
            .any(|source| source.source_tier == "secondary"));
        assert!(outcome
            .sources
            .iter()
            .any(|source| source.source_tier == "community"));
    }
}
