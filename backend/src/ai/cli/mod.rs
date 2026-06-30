use std::process::Command;

use async_trait::async_trait;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{
    ai::{
        prompt::{
            extract_json_object, investment_system_refinement_prompt, memo_extraction_prompt,
            portfolio_review_prompt, research_distillation_prompt, stock_snapshot_prompt,
        },
        AiError, AiProvider, InvestmentSystemRefinement, MemoExtraction, PortfolioReviewContext,
        ResearchAnalysis, ResearchSourceInput, StockSnapshotContext,
    },
    investment_system::InvestmentSystem,
    locale::Locale,
    memo::Memo,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CliProviderKind {
    Codex,
}

impl CliProviderKind {
    pub fn parse(value: &str) -> Result<Self, AiError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "codex" | "codex_cli" => Ok(Self::Codex),
            other => Err(AiError::Provider(format!(
                "unsupported CLI provider '{other}'. Use codex"
            ))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
        }
    }

    pub fn login_command(self, settings: &CliSettings) -> Option<String> {
        match self {
            Self::Codex => Some(format!("{} login --device-auth", settings.path)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliSettings {
    pub provider: CliProviderKind,
    pub path: String,
    pub model: Option<String>,
    pub profile: Option<String>,
}

impl Default for CliSettings {
    fn default() -> Self {
        Self {
            provider: CliProviderKind::Codex,
            path: "codex".to_string(),
            model: None,
            profile: None,
        }
    }
}

pub trait CliBackend: Clone + Send + Sync + 'static {
    fn kind(&self) -> CliProviderKind;
    fn build_command(&self, settings: &CliSettings, prompt: String) -> CliCommand;
    fn auth_hint(&self, settings: &CliSettings) -> String;
}

#[derive(Debug, Clone)]
pub struct CliCommand {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CliAiProvider<B>
where
    B: CliBackend,
{
    backend: B,
    settings: CliSettings,
}

impl<B> CliAiProvider<B>
where
    B: CliBackend,
{
    pub fn new(backend: B, settings: CliSettings) -> Self {
        Self { backend, settings }
    }

    async fn run_json<T>(&self, prompt: String) -> Result<T, AiError>
    where
        T: DeserializeOwned + Send + 'static,
    {
        let backend = self.backend.clone();
        let settings = self.settings.clone();

        tokio::task::spawn_blocking(move || run_cli_json(&backend, &settings, prompt))
            .await
            .map_err(|err| AiError::Provider(err.to_string()))?
    }
}

#[async_trait]
impl<B> AiProvider for CliAiProvider<B>
where
    B: CliBackend,
{
    async fn extract_memo(&self, memo: &Memo, locale: Locale) -> Result<MemoExtraction, AiError> {
        self.run_json(memo_extraction_prompt(memo, locale)).await
    }

    async fn refine_system(
        &self,
        system: &InvestmentSystem,
        locale: Locale,
    ) -> Result<InvestmentSystemRefinement, AiError> {
        self.run_json(investment_system_refinement_prompt(system, locale))
            .await
    }

    async fn distill_research_source(
        &self,
        input: &ResearchSourceInput,
        locale: Locale,
    ) -> Result<ResearchAnalysis, AiError> {
        self.run_json(research_distillation_prompt(input, locale))
            .await
    }

    async fn analyze_stock_snapshot(
        &self,
        context: &StockSnapshotContext,
        locale: Locale,
    ) -> Result<ResearchAnalysis, AiError> {
        self.run_json(stock_snapshot_prompt(context, locale)).await
    }

    async fn review_portfolio_risk(
        &self,
        context: &PortfolioReviewContext,
        locale: Locale,
    ) -> Result<ResearchAnalysis, AiError> {
        self.run_json(portfolio_review_prompt(context, locale))
            .await
    }
}

fn run_cli_json<T, B>(backend: &B, settings: &CliSettings, prompt: String) -> Result<T, AiError>
where
    T: DeserializeOwned,
    B: CliBackend,
{
    let command_spec = backend.build_command(settings, prompt);
    let output = Command::new(&command_spec.program)
        .args(command_spec.args)
        .output()
        .map_err(|err| {
            AiError::Provider(format!(
                "failed to run {} CLI. {} Error: {err}",
                backend.kind().as_str(),
                backend.auth_hint(settings)
            ))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AiError::Provider(format!(
            "{} CLI failed. {} stderr: {stderr}",
            backend.kind().as_str(),
            backend.auth_hint(settings)
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json = extract_json_object(stdout.trim()).ok_or_else(|| {
        AiError::Provider(format!(
            "{} CLI did not return a JSON object",
            backend.kind().as_str()
        ))
    })?;

    serde_json::from_str(json).map_err(|err| {
        AiError::Provider(format!(
            "failed to parse {} CLI JSON response: {err}. response: {json}",
            backend.kind().as_str()
        ))
    })
}

pub mod codex;
