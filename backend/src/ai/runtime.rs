use std::{
    fs,
    path::{Path, PathBuf},
    sync::RwLock,
};

use serde::{Deserialize, Serialize};

use crate::{
    ai::{
        cli::{CliProviderKind, CliSettings},
        provider_from_settings, AiError, InvestmentSystemRefinement, MemoExtraction,
        PortfolioReviewContext, ResearchAnalysis, ResearchSourceInput, StockSnapshotContext,
    },
    config::AppConfig,
    investment_system::InvestmentSystem,
    locale::Locale,
    memo::Memo,
};

#[derive(Debug)]
pub struct AiRuntime {
    settings: RwLock<AiSettings>,
    env_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiProviderKind {
    Mock,
    OpenAi,
    Cli,
}

impl AiProviderKind {
    pub fn parse(value: &str) -> Result<Self, AiError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "mock" => Ok(Self::Mock),
            "openai" | "openai_compatible" => Ok(Self::OpenAi),
            "cli" | "codex" | "codex_cli" => Ok(Self::Cli),
            other => Err(AiError::Provider(format!(
                "unsupported AI provider '{other}'. Use mock, openai, or cli"
            ))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mock => "mock",
            Self::OpenAi => "openai",
            Self::Cli => "cli",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSettings {
    pub provider: AiProviderKind,
    pub openai_api_key: Option<String>,
    pub openai_base_url: String,
    pub openai_model: String,
    pub cli: CliSettings,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiSettingsResponse {
    pub provider: String,
    pub openai_base_url: String,
    pub openai_model: String,
    pub has_openai_api_key: bool,
    pub cli_provider: String,
    pub cli_path: String,
    pub cli_model: Option<String>,
    pub cli_profile: Option<String>,
    pub cli_login_command: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateAiSettingsRequest {
    pub provider: Option<String>,
    pub openai_api_key: Option<String>,
    pub openai_base_url: Option<String>,
    pub openai_model: Option<String>,
    pub cli_provider: Option<String>,
    pub cli_path: Option<String>,
    pub cli_model: Option<String>,
    pub cli_profile: Option<String>,
    pub persist_to_env: Option<bool>,
}

impl AiRuntime {
    pub fn new(settings: AiSettings, env_path: impl Into<PathBuf>) -> Self {
        Self {
            settings: RwLock::new(settings),
            env_path: env_path.into(),
        }
    }

    pub fn from_config(config: &AppConfig) -> Self {
        Self::new(AiSettings::from_config(config), ".env")
    }

    pub fn settings_response(&self) -> AiSettingsResponse {
        self.settings()
    }

    pub fn settings(&self) -> AiSettingsResponse {
        self.settings
            .read()
            .expect("ai settings lock poisoned")
            .to_response()
    }

    pub fn update(&self, request: UpdateAiSettingsRequest) -> Result<AiSettingsResponse, AiError> {
        let mut settings = self.settings.write().expect("ai settings lock poisoned");

        if let Some(provider) = request.provider.and_then(clean_option) {
            settings.provider = AiProviderKind::parse(&provider)?;
            if provider.eq_ignore_ascii_case("codex") || provider.eq_ignore_ascii_case("codex_cli")
            {
                settings.cli.provider = CliProviderKind::Codex;
            }
        }
        if let Some(openai_api_key) = request.openai_api_key.and_then(clean_option) {
            settings.openai_api_key = Some(openai_api_key);
        }
        if let Some(openai_base_url) = request.openai_base_url.and_then(clean_option) {
            settings.openai_base_url = openai_base_url;
        }
        if let Some(openai_model) = request.openai_model.and_then(clean_option) {
            settings.openai_model = openai_model;
        }
        if let Some(cli_provider) = request.cli_provider.and_then(clean_option) {
            settings.cli.provider = CliProviderKind::parse(&cli_provider)?;
        }

        if let Some(cli_path) = request.cli_path.and_then(clean_option) {
            settings.cli.path = cli_path;
        }

        if let Some(cli_model) = request.cli_model {
            settings.cli.model = clean_option(cli_model);
        }

        if let Some(cli_profile) = request.cli_profile {
            settings.cli.profile = clean_option(cli_profile);
        }

        if request.persist_to_env.unwrap_or(false) {
            write_env_file(&self.env_path, &settings)?;
        }

        Ok(settings.to_response())
    }

    pub async fn extract_memo(
        &self,
        memo: &Memo,
        locale: Locale,
    ) -> Result<MemoExtraction, AiError> {
        let settings = self
            .settings
            .read()
            .expect("ai settings lock poisoned")
            .clone();
        provider_from_settings(&settings)
            .extract_memo(memo, locale)
            .await
    }

    pub async fn refine_system(
        &self,
        system: &InvestmentSystem,
        locale: Locale,
    ) -> Result<InvestmentSystemRefinement, AiError> {
        let settings = self
            .settings
            .read()
            .expect("ai settings lock poisoned")
            .clone();
        provider_from_settings(&settings)
            .refine_system(system, locale)
            .await
    }

    pub async fn distill_research_source(
        &self,
        input: &ResearchSourceInput,
        locale: Locale,
    ) -> Result<ResearchAnalysis, AiError> {
        let settings = self
            .settings
            .read()
            .expect("ai settings lock poisoned")
            .clone();
        provider_from_settings(&settings)
            .distill_research_source(input, locale)
            .await
    }

    pub async fn analyze_stock_snapshot(
        &self,
        context: &StockSnapshotContext,
        locale: Locale,
    ) -> Result<ResearchAnalysis, AiError> {
        let settings = self
            .settings
            .read()
            .expect("ai settings lock poisoned")
            .clone();
        provider_from_settings(&settings)
            .analyze_stock_snapshot(context, locale)
            .await
    }

    pub async fn review_portfolio_risk(
        &self,
        context: &PortfolioReviewContext,
        locale: Locale,
    ) -> Result<ResearchAnalysis, AiError> {
        let settings = self
            .settings
            .read()
            .expect("ai settings lock poisoned")
            .clone();
        provider_from_settings(&settings)
            .review_portfolio_risk(context, locale)
            .await
    }
}

impl AiSettings {
    pub fn from_config(config: &AppConfig) -> Self {
        Self {
            provider: AiProviderKind::parse(&config.ai_provider).unwrap_or(AiProviderKind::Mock),
            openai_api_key: config.openai_api_key.clone(),
            openai_base_url: config.openai_base_url.clone(),
            openai_model: config.openai_model.clone(),
            cli: CliSettings {
                provider: CliProviderKind::parse(&config.ai_cli_provider)
                    .unwrap_or(CliProviderKind::Codex),
                path: config.ai_cli_path.clone(),
                model: config.ai_cli_model.clone(),
                profile: config.ai_cli_profile.clone(),
            },
        }
    }

    pub fn to_response(&self) -> AiSettingsResponse {
        AiSettingsResponse {
            provider: self.provider.as_str().to_string(),
            openai_base_url: self.openai_base_url.clone(),
            openai_model: self.openai_model.clone(),
            has_openai_api_key: self.openai_api_key.is_some(),
            cli_provider: self.cli.provider.as_str().to_string(),
            cli_path: self.cli.path.clone(),
            cli_model: self.cli.model.clone(),
            cli_profile: self.cli.profile.clone(),
            cli_login_command: self.cli.provider.login_command(&self.cli),
        }
    }
}

fn write_env_file(path: &Path, settings: &AiSettings) -> Result<(), AiError> {
    let current = fs::read_to_string(path).unwrap_or_default();
    let mut lines = current.lines().map(ToOwned::to_owned).collect::<Vec<_>>();

    set_env_line(&mut lines, "AI_PROVIDER", settings.provider.as_str());
    set_env_line(
        &mut lines,
        "OPENAI_API_KEY",
        settings.openai_api_key.as_deref().unwrap_or_default(),
    );
    set_env_line(&mut lines, "OPENAI_BASE_URL", &settings.openai_base_url);
    set_env_line(&mut lines, "OPENAI_MODEL", &settings.openai_model);
    set_env_line(
        &mut lines,
        "AI_CLI_PROVIDER",
        settings.cli.provider.as_str(),
    );
    set_env_line(&mut lines, "AI_CLI_PATH", &settings.cli.path);
    set_env_line(
        &mut lines,
        "AI_CLI_MODEL",
        settings.cli.model.as_deref().unwrap_or_default(),
    );
    set_env_line(
        &mut lines,
        "AI_CLI_PROFILE",
        settings.cli.profile.as_deref().unwrap_or_default(),
    );

    fs::write(path, format!("{}\n", lines.join("\n")))
        .map_err(|err| AiError::Provider(format!("failed to write .env: {err}")))?;

    Ok(())
}

fn set_env_line(lines: &mut Vec<String>, key: &str, value: &str) {
    let prefix = format!("{key}=");
    let replacement = format!("{key}={}", escape_env_value(value));

    if let Some(line) = lines
        .iter_mut()
        .find(|line| line.trim_start().starts_with(&prefix))
    {
        *line = replacement;
    } else {
        lines.push(replacement);
    }
}

fn escape_env_value(value: &str) -> String {
    if value.contains([' ', '#', '"', '\'']) {
        format!("\"{}\"", value.replace('"', "\\\""))
    } else {
        value.to_string()
    }
}

fn clean_option(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
