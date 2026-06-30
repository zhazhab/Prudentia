use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    config::AppConfig,
    investment_system::InvestmentSystem,
    locale::Locale,
    market_data::MarketQuote,
    memo::Memo,
    portfolio::{PortfolioPosition, PortfolioSummary},
};

pub mod cli;
pub mod mock;
pub mod openai;
pub(crate) mod prompt;
pub mod runtime;

#[async_trait]
pub trait AiProvider: Send + Sync {
    async fn extract_memo(&self, memo: &Memo, locale: Locale) -> Result<MemoExtraction, AiError>;
    async fn refine_system(
        &self,
        system: &InvestmentSystem,
        locale: Locale,
    ) -> Result<InvestmentSystemRefinement, AiError>;
    async fn distill_research_source(
        &self,
        input: &ResearchSourceInput,
        locale: Locale,
    ) -> Result<ResearchAnalysis, AiError>;
    async fn analyze_stock_snapshot(
        &self,
        context: &StockSnapshotContext,
        locale: Locale,
    ) -> Result<ResearchAnalysis, AiError>;
    async fn review_portfolio_risk(
        &self,
        context: &PortfolioReviewContext,
        locale: Locale,
    ) -> Result<ResearchAnalysis, AiError>;
}

#[derive(Debug, Error)]
pub enum AiError {
    #[error("{0}")]
    Provider(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoExtraction {
    pub thesis: String,
    pub risks: String,
    pub catalysts: String,
    pub disconfirming_evidence: String,
    pub checklist: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvestmentSystemRefinement {
    pub principles: Vec<String>,
    pub checklist_items: Vec<String>,
    pub circle_of_competence: Vec<String>,
    pub decision_rules: Vec<String>,
    pub summary: String,
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

#[derive(Debug, Clone, Serialize)]
pub struct ResearchSourceInput {
    pub title: String,
    pub source_type: Option<String>,
    pub source_title: Option<String>,
    pub source_author: Option<String>,
    pub source_content: String,
    pub symbol: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StockSnapshotContext {
    pub symbol: String,
    pub position: Option<PortfolioPosition>,
    pub portfolio_summary: PortfolioSummary,
    pub related_memos: Vec<Memo>,
    pub selected_memo: Option<Memo>,
    pub quote: Option<MarketQuote>,
    pub quote_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PortfolioReviewContext {
    pub positions: Vec<PortfolioPosition>,
    pub summary: PortfolioSummary,
    pub holdings_without_memo: Vec<String>,
}

pub fn provider_from_config(config: &AppConfig) -> Arc<dyn AiProvider> {
    let settings = runtime::AiSettings::from_config(config);
    provider_from_settings(&settings)
}

pub fn provider_from_settings(settings: &runtime::AiSettings) -> Arc<dyn AiProvider> {
    match settings.provider {
        runtime::AiProviderKind::Cli => {
            return match settings.cli.provider {
                cli::CliProviderKind::Codex => Arc::new(cli::CliAiProvider::new(
                    cli::codex::CodexCliBackend,
                    settings.cli.clone(),
                )),
            };
        }
        runtime::AiProviderKind::OpenAi => {
            if let Some(api_key) = &settings.openai_api_key {
                return Arc::new(openai::OpenAiCompatibleProvider::new(
                    settings.openai_base_url.clone(),
                    api_key.clone(),
                    settings.openai_model.clone(),
                ));
            }

            tracing::warn!(
                "AI_PROVIDER=openai was set without OPENAI_API_KEY; falling back to mock AI"
            );
        }
        runtime::AiProviderKind::Mock => {}
    }

    Arc::new(mock::MockAiProvider)
}
