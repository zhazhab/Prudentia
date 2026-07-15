use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use tokio::sync::mpsc;

use crate::{
    ai::ConversationResearchSource,
    config::{AppConfig, WebResearchProviderKind},
    error::{AppError, AppResult},
    time::now_iso,
};

mod planner;
mod public_sources;
mod source_validation;
use planner::EvidenceCategory;
pub(super) use planner::{plan_research, ResearchPlan};
use source_validation::{normalize_company_sources, normalize_sources, verify_source_urls};

const RESEARCH_CACHE_TTL: chrono::Duration = chrono::Duration::hours(24);
const PUBLIC_SOURCES_CACHE_VERSION: &str = "v6-evidence-quality";

#[async_trait]
pub trait WebResearchProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn enabled(&self) -> bool;
    /// Must change whenever any plan input or provider behavior can change the persisted outcome.
    fn cache_identity(&self, plan: &ResearchPlan) -> String {
        plan.query_cache_identity()
    }
    async fn execute(
        &self,
        plan: &ResearchPlan,
        progress: mpsc::UnboundedSender<ResearchProgress>,
    ) -> Result<ResearchOutcome, ResearchError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResearchProgress {
    CheckingCache,
    CacheHit,
    FetchingPublicSources,
    FetchingFinancialHistory,
    VerifyingSources,
    SearchingOfficial,
    SearchingIndependent,
    SearchingCommunity,
}

impl ResearchProgress {
    pub(super) fn code(self) -> &'static str {
        match self {
            Self::CheckingCache => "research_checking_cache",
            Self::CacheHit => "research_cache_hit",
            Self::FetchingPublicSources => "research_fetching_public_sources",
            Self::FetchingFinancialHistory => "research_fetching_financial_history",
            Self::VerifyingSources => "research_verifying_sources",
            Self::SearchingOfficial => "research_searching_official",
            Self::SearchingIndependent => "research_searching_independent",
            Self::SearchingCommunity => "research_searching_community",
        }
    }
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

impl EvidenceCategory {
    fn accepts(self, source: &ConversationResearchSource) -> bool {
        match self {
            Self::Official => source.source_tier == "primary",
            Self::Independent => source.source_tier == "secondary",
            Self::Community => source.source_tier == "community",
        }
    }
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

pub(super) async fn execute_with_cache(
    pool: &SqlitePool,
    provider: Arc<dyn WebResearchProvider>,
    plan: &ResearchPlan,
    progress: mpsc::UnboundedSender<ResearchProgress>,
) -> AppResult<ResearchOutcome> {
    let _ = progress.send(ResearchProgress::CheckingCache);
    prune_expired_cache(pool).await?;
    let cache_key = format!("{}:{}", provider.name(), provider.cache_identity(plan));
    let hash = query_hash(&cache_key);
    if let Some(cached) = load_cache(pool, &hash)
        .await?
        .filter(|outcome| !outcome.sources.is_empty())
    {
        let _ = progress.send(ResearchProgress::CacheHit);
        return Ok(cached);
    }
    if !provider.enabled() {
        return Err(AppError::bad_request(
            "external research provider is not configured",
        ));
    }
    let outcome = provider
        .execute(plan, progress)
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

struct DisabledResearchProvider;

#[async_trait]
impl WebResearchProvider for DisabledResearchProvider {
    fn name(&self) -> &'static str {
        "disabled"
    }

    fn enabled(&self) -> bool {
        false
    }

    async fn execute(
        &self,
        _plan: &ResearchPlan,
        _progress: mpsc::UnboundedSender<ResearchProgress>,
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

    async fn execute(
        &self,
        _plan: &ResearchPlan,
        _progress: mpsc::UnboundedSender<ResearchProgress>,
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

    fn cache_identity(&self, plan: &ResearchPlan) -> String {
        format!(
            "{PUBLIC_SOURCES_CACHE_VERSION}:{}",
            plan.subject_cache_identity()
        )
    }

    async fn execute(
        &self,
        plan: &ResearchPlan,
        progress: mpsc::UnboundedSender<ResearchProgress>,
    ) -> Result<ResearchOutcome, ResearchError> {
        let progress_stage = if plan.annual_history_years().is_some() {
            ResearchProgress::FetchingFinancialHistory
        } else {
            ResearchProgress::FetchingPublicSources
        };
        let _ = progress.send(progress_stage);
        let (official, independent, community) = tokio::join!(
            public_sources::official_filings(&self.client, plan),
            public_sources::independent_news(&self.client, plan),
            public_sources::community_discussions(&self.client, plan),
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

        let _ = progress.send(ResearchProgress::VerifyingSources);
        let (mut verified_company_facts, official_to_verify): (Vec<_>, Vec<_>) =
            normalize_sources(official).into_iter().partition(|source| {
                source
                    .url
                    .starts_with("https://data.sec.gov/api/xbrl/companyfacts/")
            });
        let (verified_official, independent, community) = tokio::join!(
            verify_source_urls(official_to_verify),
            verify_source_urls(normalize_sources(independent)),
            verify_source_urls(normalize_sources(community)),
        );
        verified_company_facts.extend(verified_official);
        let official = verified_company_facts;
        let official_incomplete = official.is_empty()
            || (plan.annual_history_years().is_some()
                && !official
                    .iter()
                    .any(|source| source.url.contains("/api/xbrl/companyfacts/")));
        for (category, is_empty) in [
            (EvidenceCategory::Official, official_incomplete),
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

    async fn execute(
        &self,
        plan: &ResearchPlan,
        progress: mpsc::UnboundedSender<ResearchProgress>,
    ) -> Result<ResearchOutcome, ResearchError> {
        let mut sources = Vec::new();
        let mut failed_categories = Vec::new();
        for planned_query in plan.queries() {
            let stage = match planned_query.category {
                EvidenceCategory::Official => ResearchProgress::SearchingOfficial,
                EvidenceCategory::Independent => ResearchProgress::SearchingIndependent,
                EvidenceCategory::Community => ResearchProgress::SearchingCommunity,
            };
            let _ = progress.send(stage);
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
                plan.company_name(),
                plan.symbol(),
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

async fn prune_expired_cache(pool: &SqlitePool) -> AppResult<()> {
    let cutoff = (chrono::Utc::now() - RESEARCH_CACHE_TTL).to_rfc3339();
    sqlx::query(
        r#"DELETE FROM conversation_research_cache
        WHERE julianday(fetched_at) IS NULL OR julianday(fetched_at) < julianday(?)"#,
    )
    .bind(cutoff)
    .execute(pool)
    .await?;
    Ok(())
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
        incomplete_research_warning, plan_research, prune_expired_cache, require_usable_sources,
        DisabledResearchProvider, EvidenceCategory, MisconfiguredResearchProvider,
        PublicSourcesResearchProvider, ResearchError, TavilyResearchProvider, WebResearchProvider,
        RESEARCH_CACHE_TTL,
    };
    use crate::conversation::types::ThreadSubject;

    fn pdd_subject() -> ThreadSubject {
        ThreadSubject {
            kind: "company".to_string(),
            subject_key: Some("PDD".to_string()),
            label: Some("PDD Holdings".to_string()),
            confidence: 0.95,
        }
    }

    #[test]
    fn provider_cache_identity_matches_its_execution_scope() {
        let earnings = plan_research("分析 PDD 最新财报", &pdd_subject()).expect("earnings plan");
        let valuation = plan_research("分析 PDD 当前估值", &pdd_subject()).expect("valuation plan");
        let history = plan_research("研究 PDD 近五年财报", &pdd_subject()).expect("history plan");
        let public_sources = PublicSourcesResearchProvider {
            client: reqwest::Client::new(),
        };
        let tavily = TavilyResearchProvider {
            client: reqwest::Client::new(),
            api_key: "test-key".to_string(),
        };

        assert_eq!(
            public_sources.cache_identity(&earnings),
            public_sources.cache_identity(&valuation)
        );
        assert_ne!(
            public_sources.cache_identity(&earnings),
            public_sources.cache_identity(&history)
        );
        assert_ne!(
            tavily.cache_identity(&earnings),
            tavily.cache_identity(&valuation)
        );
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
    async fn expired_research_cache_rows_are_pruned() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("cache database");
        sqlx::query(
            r#"CREATE TABLE conversation_research_cache (
                query_hash TEXT PRIMARY KEY,
                query TEXT NOT NULL,
                results_json TEXT NOT NULL,
                fetched_at TEXT NOT NULL
            )"#,
        )
        .execute(&pool)
        .await
        .expect("cache schema");
        for (hash, fetched_at) in [
            (
                "expired",
                (chrono::Utc::now() - chrono::Duration::hours(25)).to_rfc3339(),
            ),
            ("fresh", chrono::Utc::now().to_rfc3339()),
        ] {
            sqlx::query("INSERT INTO conversation_research_cache VALUES (?, ?, '{}', ?)")
                .bind(hash)
                .bind(hash)
                .bind(fetched_at)
                .execute(&pool)
                .await
                .expect("cache row");
        }

        prune_expired_cache(&pool).await.expect("prune cache");

        let hashes = sqlx::query_scalar::<_, String>(
            "SELECT query_hash FROM conversation_research_cache ORDER BY query_hash",
        )
        .fetch_all(&pool)
        .await
        .expect("remaining cache rows");
        assert_eq!(hashes, vec!["fresh"]);
    }

    #[tokio::test]
    async fn only_explicit_disabled_configuration_turns_research_off() {
        let disabled = DisabledResearchProvider;
        assert!(!disabled.enabled());

        let invalid = MisconfiguredResearchProvider {
            reason: "unsupported provider".to_string(),
        };
        assert!(invalid.enabled());
        let plan = plan_research("分析 PDD", &pdd_subject()).expect("research plan");
        let (progress, _events) = tokio::sync::mpsc::unbounded_channel();
        let error = invalid
            .execute(&plan, progress)
            .await
            .expect_err("invalid config");
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
        let plan =
            plan_research("PDD 近五年财报和市场观点", &pdd_subject()).expect("research plan");
        let provider: Arc<dyn WebResearchProvider> = Arc::new(PublicSourcesResearchProvider {
            client: reqwest::Client::new(),
        });

        let (progress, _events) = tokio::sync::mpsc::unbounded_channel();
        let outcome = provider
            .execute(&plan, progress)
            .await
            .expect("live public sources");
        for source in &outcome.sources {
            println!("{} | {} | {}", source.source_tier, source.title, source.url);
        }

        assert!(outcome
            .sources
            .iter()
            .any(|source| source.source_tier == "primary" && source.url.contains("sec.gov")));
        assert!(outcome.sources.iter().any(|source| {
            source.url.contains("/api/xbrl/companyfacts/")
                && source.snippet.contains("2021: revenue")
                && source.snippet.contains("2025: revenue")
                && source.snippet.contains("gross profit")
                && source.snippet.contains("operating income")
                && source.snippet.contains("selling and marketing expense")
                && source.snippet.contains("capital expenditure")
                && source.snippet.contains("share-based compensation")
                && source.snippet.contains("diluted weighted-average shares")
                && source
                    .snippet
                    .contains("free-cash-flow proxy per diluted share")
                && source.snippet.contains(
                    "Excluded diluted weighted-average shares for 2022 as an isolated scale outlier"
                )
                && !source
                    .snippet
                    .contains("diluted weighted-average shares 5.761 million")
        }));
        assert!(outcome
            .sources
            .iter()
            .filter(|source| source.source_tier == "primary")
            .any(|source| source.snippet.contains("Filing excerpt:")));
        assert!(outcome.sources.iter().any(|source| {
            source.title.contains("SEC 20-F")
                && source.url.contains("/Archives/edgar/data/")
                && source.snippet.contains("Business Overview")
                && source.snippet.contains("third-party merchants")
                && source.snippet.contains("transaction services")
                && source.snippet.contains("Competition evidence:")
                && source.snippet.contains("Profit-engine evidence:")
                && source.snippet.contains("Owner-economics evidence:")
                && source
                    .snippet
                    .contains("Management, incentives, and capital-allocation evidence:")
                && source.snippet.contains("Financial-resilience evidence:")
                && !source.snippet.contains("XBRL Viewer")
        }));
        let has_secondary = outcome
            .sources
            .iter()
            .any(|source| source.source_tier == "secondary");
        assert!(
            has_secondary
                || outcome
                    .warning
                    .as_deref()
                    .is_some_and(|warning| warning.contains("independent analysis"))
        );
        let has_community = outcome
            .sources
            .iter()
            .any(|source| source.source_tier == "community");
        assert!(
            has_community
                || outcome
                    .warning
                    .as_deref()
                    .is_some_and(|warning| warning.contains("community viewpoints"))
        );
    }
}
