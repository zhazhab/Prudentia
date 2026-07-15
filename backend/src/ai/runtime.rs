use std::{
    fs,
    path::{Path, PathBuf},
    sync::RwLock,
};

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::{
    ai::{
        cli::{CliProviderKind, CliSettings},
        provider_for_kind, provider_from_settings, AiError, AiProviderEvent, ConversationContext,
        ConversationProjection, InvestmentSystemRefinement, MemoChatContext, MemoExtraction,
        PortfolioImageRecognition, PortfolioReviewContext, ResearchAnalysis, ResearchSourceInput,
        StockSnapshotContext,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskComplexity {
    Simple,
    Standard,
    Deep,
}

impl TaskComplexity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Simple => "simple",
            Self::Standard => "standard",
            Self::Deep => "deep",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelTierSettings {
    pub simple: String,
    pub standard: String,
    pub deep: String,
}

impl ModelTierSettings {
    fn model_for(&self, complexity: TaskComplexity) -> &str {
        match complexity {
            TaskComplexity::Simple => &self.simple,
            TaskComplexity::Standard => &self.standard,
            TaskComplexity::Deep => &self.deep,
        }
    }

    fn set_all(&mut self, model: String) {
        self.simple = model.clone();
        self.standard = model.clone();
        self.deep = model;
    }
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
    pub provider_chain: Vec<AiProviderKind>,
    pub openai_api_key: Option<String>,
    pub openai_base_url: String,
    pub openai_model: String,
    pub openai_models: ModelTierSettings,
    pub cli: CliSettings,
    pub cli_models: ModelTierSettings,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiSettingsResponse {
    pub provider: String,
    pub provider_chain: Vec<String>,
    pub openai_base_url: String,
    pub openai_model: String,
    pub openai_model_simple: String,
    pub openai_model_standard: String,
    pub openai_model_deep: String,
    pub has_openai_api_key: bool,
    pub cli_provider: String,
    pub cli_path: String,
    pub cli_model: Option<String>,
    pub cli_model_simple: String,
    pub cli_model_standard: String,
    pub cli_model_deep: String,
    pub cli_profile: Option<String>,
    pub cli_login_command: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateAiSettingsRequest {
    pub provider: Option<String>,
    pub openai_api_key: Option<String>,
    pub openai_base_url: Option<String>,
    pub openai_model: Option<String>,
    pub openai_model_simple: Option<String>,
    pub openai_model_standard: Option<String>,
    pub openai_model_deep: Option<String>,
    pub cli_provider: Option<String>,
    pub cli_path: Option<String>,
    pub cli_model: Option<String>,
    pub cli_model_simple: Option<String>,
    pub cli_model_standard: Option<String>,
    pub cli_model_deep: Option<String>,
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
        Self::new(
            AiSettings::from_config(config),
            crate::config::LocalAppPaths::discover().env_path,
        )
    }

    pub fn from_config_with_env_path(config: &AppConfig, env_path: impl Into<PathBuf>) -> Self {
        Self::new(AiSettings::from_config(config), env_path)
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
            settings.provider_chain = parse_provider_chain(&provider)?;
            settings.provider = settings.provider_chain[0];
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
            settings.openai_model = openai_model.clone();
            settings.openai_models.set_all(openai_model);
        }
        if let Some(model) = request.openai_model_simple.and_then(clean_option) {
            settings.openai_models.simple = model;
        }
        if let Some(model) = request.openai_model_standard.and_then(clean_option) {
            settings.openai_model = model.clone();
            settings.openai_models.standard = model;
        }
        if let Some(model) = request.openai_model_deep.and_then(clean_option) {
            settings.openai_models.deep = model;
        }
        if let Some(cli_provider) = request.cli_provider.and_then(clean_option) {
            settings.cli.provider = CliProviderKind::parse(&cli_provider)?;
        }

        if let Some(cli_path) = request.cli_path.and_then(clean_option) {
            settings.cli.path = cli_path;
        }

        if let Some(cli_model) = request.cli_model {
            settings.cli.model = clean_option(cli_model);
            if let Some(model) = settings.cli.model.clone() {
                settings.cli_models.set_all(model);
            }
        }
        if let Some(model) = request.cli_model_simple.and_then(clean_option) {
            settings.cli_models.simple = model;
        }
        if let Some(model) = request.cli_model_standard.and_then(clean_option) {
            settings.cli.model = Some(model.clone());
            settings.cli_models.standard = model;
        }
        if let Some(model) = request.cli_model_deep.and_then(clean_option) {
            settings.cli_models.deep = model;
        }

        if let Some(cli_profile) = request.cli_profile {
            settings.cli.profile = clean_option(cli_profile);
        }

        if request.persist_to_env.unwrap_or(false) {
            write_env_file(&self.env_path, &settings)?;
        }

        Ok(settings.to_response())
    }

    pub async fn respond_to_memo_chat(
        &self,
        context: &MemoChatContext,
        locale: Locale,
    ) -> Result<String, AiError> {
        let settings = self
            .settings
            .read()
            .expect("ai settings lock poisoned")
            .clone();
        provider_from_settings(&settings)
            .respond_to_memo_chat(context, locale)
            .await
    }

    pub async fn respond_to_conversation(
        &self,
        context: &ConversationContext,
        locale: Locale,
        complexity: TaskComplexity,
        route_reason: &str,
        events: mpsc::UnboundedSender<AiProviderEvent>,
    ) -> Result<String, AiError> {
        let settings = self
            .settings
            .read()
            .expect("ai settings lock poisoned")
            .clone();
        let mut last_error = None;
        for (index, kind) in settings.provider_chain.iter().copied().enumerate() {
            let (routed_settings, model) = settings.routed_for(kind, complexity);
            let _ = events.send(AiProviderEvent::RouteSelected {
                provider: kind.as_str().to_string(),
                model,
                complexity: complexity.as_str().to_string(),
                reason: route_reason.to_string(),
            });
            let provider = provider_for_kind(&routed_settings, kind);
            let (provider_tx, mut provider_rx) = mpsc::unbounded_channel();
            let response = provider.respond_to_conversation(context, locale, provider_tx);
            tokio::pin!(response);
            let mut content_started = false;
            let result = loop {
                tokio::select! {
                    event = provider_rx.recv() => {
                        let Some(event) = event else {
                            break response.await;
                        };
                        if matches!(event, AiProviderEvent::TextDelta(_)) {
                            content_started = true;
                        }
                        let _ = events.send(event);
                    }
                    result = &mut response => break result,
                }
            };
            while let Ok(event) = provider_rx.try_recv() {
                if matches!(event, AiProviderEvent::TextDelta(_)) {
                    content_started = true;
                }
                let _ = events.send(event);
            }
            match result {
                Ok(response) => return Ok(response),
                Err(error) if !content_started && index + 1 < settings.provider_chain.len() => {
                    last_error = Some(error);
                    let _ = events.send(AiProviderEvent::Stage {
                        provider: kind.as_str().to_string(),
                        stage: "provider_fallback".to_string(),
                    });
                }
                Err(error) => return Err(error),
            }
        }
        Err(last_error
            .unwrap_or_else(|| AiError::Provider("no AI provider is configured".to_string())))
    }

    pub async fn project_conversation(
        &self,
        context: &ConversationContext,
        assistant_response: &str,
        locale: Locale,
        complexity: TaskComplexity,
    ) -> Result<ConversationProjection, AiError> {
        let settings = self
            .settings
            .read()
            .expect("ai settings lock poisoned")
            .clone();
        let mut errors = Vec::new();
        for kind in &settings.provider_chain {
            let (routed_settings, _) = settings.routed_for(*kind, complexity);
            match provider_for_kind(&routed_settings, *kind)
                .project_conversation(context, assistant_response, locale)
                .await
            {
                Ok(projection) => return Ok(projection),
                Err(error) => errors.push(format!("{}: {error}", kind.as_str())),
            }
        }
        Err(AiError::Provider(format!(
            "all configured AI providers failed to project the turn: {}",
            errors.join("; ")
        )))
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

    pub async fn recognize_portfolio_image(
        &self,
        image_path: &Path,
    ) -> Result<PortfolioImageRecognition, AiError> {
        let settings = self
            .settings
            .read()
            .expect("ai settings lock poisoned")
            .clone();
        provider_from_settings(&settings)
            .recognize_portfolio_image(image_path)
            .await
    }
}

impl AiSettings {
    pub fn from_config(config: &AppConfig) -> Self {
        let provider_chain =
            parse_provider_chain(&config.ai_provider).unwrap_or_else(|_| vec![AiProviderKind::Cli]);
        Self {
            provider: provider_chain[0],
            provider_chain,
            openai_api_key: config.openai_api_key.clone(),
            openai_base_url: config.openai_base_url.clone(),
            openai_model: config.openai_model.clone(),
            openai_models: ModelTierSettings {
                simple: config.openai_model_simple.clone(),
                standard: config.openai_model_standard.clone(),
                deep: config.openai_model_deep.clone(),
            },
            cli: CliSettings {
                provider: CliProviderKind::parse(&config.ai_cli_provider)
                    .unwrap_or(CliProviderKind::Codex),
                path: config.ai_cli_path.clone(),
                model: config.ai_cli_model.clone(),
                profile: config.ai_cli_profile.clone(),
            },
            cli_models: ModelTierSettings {
                simple: config.ai_cli_model_simple.clone(),
                standard: config.ai_cli_model_standard.clone(),
                deep: config.ai_cli_model_deep.clone(),
            },
        }
    }

    fn routed_for(&self, provider: AiProviderKind, complexity: TaskComplexity) -> (Self, String) {
        let mut settings = self.clone();
        let model = match provider {
            AiProviderKind::Cli => {
                let model = self.cli_models.model_for(complexity).to_string();
                settings.cli.model = Some(model.clone());
                model
            }
            AiProviderKind::OpenAi => {
                let model = self.openai_models.model_for(complexity).to_string();
                settings.openai_model = model.clone();
                model
            }
            AiProviderKind::Mock => "mock".to_string(),
        };
        (settings, model)
    }

    pub fn to_response(&self) -> AiSettingsResponse {
        AiSettingsResponse {
            provider: self.provider.as_str().to_string(),
            provider_chain: self
                .provider_chain
                .iter()
                .map(|provider| provider.as_str().to_string())
                .collect(),
            openai_base_url: self.openai_base_url.clone(),
            openai_model: self.openai_model.clone(),
            openai_model_simple: self.openai_models.simple.clone(),
            openai_model_standard: self.openai_models.standard.clone(),
            openai_model_deep: self.openai_models.deep.clone(),
            has_openai_api_key: self.openai_api_key.is_some(),
            cli_provider: self.cli.provider.as_str().to_string(),
            cli_path: self.cli.path.clone(),
            cli_model: self.cli.model.clone(),
            cli_model_simple: self.cli_models.simple.clone(),
            cli_model_standard: self.cli_models.standard.clone(),
            cli_model_deep: self.cli_models.deep.clone(),
            cli_profile: self.cli.profile.clone(),
            cli_login_command: self.cli.provider.login_command(&self.cli),
        }
    }
}

fn write_env_file(path: &Path, settings: &AiSettings) -> Result<(), AiError> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|err| {
            AiError::Provider(format!(
                "failed to create config directory {}: {err}",
                parent.display()
            ))
        })?;
    }
    let current = fs::read_to_string(path).unwrap_or_default();
    let mut lines = current.lines().map(ToOwned::to_owned).collect::<Vec<_>>();

    set_env_line(
        &mut lines,
        "AI_PROVIDER",
        &settings
            .provider_chain
            .iter()
            .map(|provider| provider.as_str())
            .collect::<Vec<_>>()
            .join(","),
    );
    set_env_line(
        &mut lines,
        "OPENAI_API_KEY",
        settings.openai_api_key.as_deref().unwrap_or_default(),
    );
    set_env_line(&mut lines, "OPENAI_BASE_URL", &settings.openai_base_url);
    set_env_line(&mut lines, "OPENAI_MODEL", &settings.openai_model);
    set_env_line(
        &mut lines,
        "OPENAI_MODEL_SIMPLE",
        &settings.openai_models.simple,
    );
    set_env_line(
        &mut lines,
        "OPENAI_MODEL_STANDARD",
        &settings.openai_models.standard,
    );
    set_env_line(
        &mut lines,
        "OPENAI_MODEL_DEEP",
        &settings.openai_models.deep,
    );
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
        "AI_CLI_MODEL_SIMPLE",
        &settings.cli_models.simple,
    );
    set_env_line(
        &mut lines,
        "AI_CLI_MODEL_STANDARD",
        &settings.cli_models.standard,
    );
    set_env_line(&mut lines, "AI_CLI_MODEL_DEEP", &settings.cli_models.deep);
    set_env_line(
        &mut lines,
        "AI_CLI_PROFILE",
        settings.cli.profile.as_deref().unwrap_or_default(),
    );

    fs::write(path, format!("{}\n", lines.join("\n"))).map_err(|err| {
        AiError::Provider(format!(
            "failed to write config file {}: {err}",
            path.display()
        ))
    })?;

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

fn parse_provider_chain(value: &str) -> Result<Vec<AiProviderKind>, AiError> {
    let mut providers = value
        .split(',')
        .map(AiProviderKind::parse)
        .collect::<Result<Vec<_>, _>>()?;
    providers.dedup();
    if providers.is_empty() {
        return Err(AiError::Provider(
            "at least one AI provider is required".to_string(),
        ));
    }
    Ok(providers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversation_routing_selects_the_configured_model_for_each_tier() {
        let settings = test_settings();

        let (simple, simple_model) =
            settings.routed_for(AiProviderKind::Cli, TaskComplexity::Simple);
        let (standard, standard_model) =
            settings.routed_for(AiProviderKind::Cli, TaskComplexity::Standard);
        let (deep, deep_model) = settings.routed_for(AiProviderKind::Cli, TaskComplexity::Deep);

        assert_eq!(simple_model, "gpt-5.6-luna");
        assert_eq!(simple.cli.model.as_deref(), Some("gpt-5.6-luna"));
        assert_eq!(standard_model, "gpt-5.6-terra");
        assert_eq!(standard.cli.model.as_deref(), Some("gpt-5.6-terra"));
        assert_eq!(deep_model, "gpt-5.6-sol");
        assert_eq!(deep.cli.model.as_deref(), Some("gpt-5.6-sol"));
    }

    #[test]
    fn openai_compatible_routing_uses_endpoint_specific_tier_models() {
        let settings = test_settings();
        let (deep, model) = settings.routed_for(AiProviderKind::OpenAi, TaskComplexity::Deep);

        assert_eq!(model, "hosted-deep");
        assert_eq!(deep.openai_model, "hosted-deep");
    }

    fn test_settings() -> AiSettings {
        AiSettings {
            provider: AiProviderKind::Cli,
            provider_chain: vec![AiProviderKind::Cli],
            openai_api_key: None,
            openai_base_url: "https://example.com/v1".to_string(),
            openai_model: "hosted-standard".to_string(),
            openai_models: ModelTierSettings {
                simple: "hosted-simple".to_string(),
                standard: "hosted-standard".to_string(),
                deep: "hosted-deep".to_string(),
            },
            cli: CliSettings::default(),
            cli_models: ModelTierSettings {
                simple: "gpt-5.6-luna".to_string(),
                standard: "gpt-5.6-terra".to_string(),
                deep: "gpt-5.6-sol".to_string(),
            },
        }
    }
}
