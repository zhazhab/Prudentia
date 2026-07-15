use std::{path::Path, sync::Arc};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tokio::sync::mpsc;

use crate::{
    config::AppConfig,
    investment_system::InvestmentSystem,
    locale::Locale,
    market_data::MarketQuote,
    memo::Memo,
    portfolio::{PortfolioPosition, PortfolioSummary},
};
pub use crate::{portfolio::PortfolioImageRecognition, research::ResearchAnalysis};

pub mod cli;
pub mod mock;
pub mod openai;
pub(crate) mod prompt;
pub mod runtime;

#[async_trait]
pub trait AiProvider: Send + Sync {
    async fn respond_to_conversation(
        &self,
        context: &ConversationContext,
        locale: Locale,
        events: mpsc::UnboundedSender<AiProviderEvent>,
    ) -> Result<String, AiError>;
    async fn project_conversation(
        &self,
        context: &ConversationContext,
        assistant_response: &str,
        locale: Locale,
    ) -> Result<ConversationProjection, AiError>;
    async fn respond_to_memo_chat(
        &self,
        context: &MemoChatContext,
        locale: Locale,
    ) -> Result<String, AiError>;
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
    async fn recognize_portfolio_image(
        &self,
        image_path: &Path,
    ) -> Result<PortfolioImageRecognition, AiError>;
}

#[derive(Debug, Error)]
pub enum AiError {
    #[error("{0}")]
    Provider(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoChatContext {
    pub thread_title: String,
    pub thread_summary: String,
    pub user_message: String,
    pub recent_messages: Vec<MemoChatHistoryMessage>,
    pub portfolio_summary: PortfolioSummary,
    pub portfolio_positions: Vec<PortfolioPosition>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoChatHistoryMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConversationContext {
    pub thread_title: String,
    pub thread_summary: String,
    pub turn_summaries: Vec<String>,
    pub subject: Value,
    pub user_message: String,
    pub recent_messages: Vec<MemoChatHistoryMessage>,
    pub portfolio_summary: PortfolioSummary,
    pub portfolio_positions: Vec<PortfolioPosition>,
    pub company_view: Option<Value>,
    pub recent_trades: Vec<Value>,
    pub investment_system: Value,
    pub attachments: Vec<ConversationAttachmentContext>,
    pub research_sources: Vec<ConversationResearchSource>,
    pub research_warning: Option<String>,
    pub subject_clarification: Option<ConversationSubjectClarification>,
    pub used_context: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConversationSubjectClarification {
    pub target_hint: Option<String>,
    pub candidates: Vec<ConversationSubjectCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConversationSubjectCandidate {
    pub symbol: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConversationAttachmentContext {
    pub id: String,
    pub file_name: String,
    pub mime_type: String,
    pub extracted_text: Option<String>,
    pub parse_status: String,
    #[serde(skip_serializing)]
    pub local_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationResearchSource {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub source_tier: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationProjection {
    pub summary: String,
    #[serde(default)]
    pub actions: Vec<ConversationActionDraft>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationActionDraft {
    pub action_type: String,
    pub title: String,
    pub rationale: String,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiProviderEvent {
    RouteSelected {
        provider: String,
        model: String,
        complexity: String,
        reason: String,
    },
    Stage {
        provider: String,
        stage: String,
    },
    TextDelta(String),
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
    provider_for_kind(settings, settings.provider)
}

pub fn provider_for_kind(
    settings: &runtime::AiSettings,
    kind: runtime::AiProviderKind,
) -> Arc<dyn AiProvider> {
    match kind {
        runtime::AiProviderKind::Cli => {
            return match settings.cli.provider {
                cli::CliProviderKind::Codex => Arc::new(cli::CliAiProvider::new(
                    cli::codex::CodexCliBackend,
                    settings.cli.clone(),
                )),
            };
        }
        runtime::AiProviderKind::OpenAi => {
            if settings.openai_api_key.is_none() {
                tracing::warn!("AI_PROVIDER=openai was set without OPENAI_API_KEY");
            }
            return Arc::new(openai::OpenAiCompatibleProvider::new(
                settings.openai_base_url.clone(),
                settings.openai_api_key.clone().unwrap_or_default(),
                settings.openai_model.clone(),
            ));
        }
        runtime::AiProviderKind::Mock => {}
    }

    Arc::new(mock::MockAiProvider)
}
