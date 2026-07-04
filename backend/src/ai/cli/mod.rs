use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    time::Instant,
};

use async_trait::async_trait;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::process::Command;
use uuid::Uuid;

use crate::{
    ai::{
        prompt::{
            extract_json_object, investment_system_refinement_prompt, memo_extraction_prompt,
            portfolio_image_recognition_prompt, portfolio_image_recognition_schema,
            portfolio_review_prompt, research_distillation_prompt, stock_snapshot_prompt,
        },
        AiError, AiProvider, InvestmentSystemRefinement, MemoExtraction, PortfolioImageRecognition,
        PortfolioReviewContext, ResearchAnalysis, ResearchSourceInput, StockSnapshotContext,
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
    fn build_image_command(
        &self,
        settings: &CliSettings,
        image_path: &Path,
        schema_path: Option<&Path>,
        prompt: String,
    ) -> CliCommand;
    fn auth_hint(&self, settings: &CliSettings) -> String;
}

#[derive(Debug, Clone)]
pub struct CliCommand {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct CliTokenUsage {
    input_tokens: Option<u64>,
    cached_input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    reasoning_output_tokens: Option<u64>,
    total_tokens: Option<u64>,
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
        run_cli_json(&self.backend, &self.settings, prompt).await
    }

    async fn run_image_json<T>(&self, image_path: PathBuf, prompt: String) -> Result<T, AiError>
    where
        T: DeserializeOwned + Send + 'static,
    {
        run_cli_image_json(&self.backend, &self.settings, &image_path, prompt).await
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

    async fn recognize_portfolio_image(
        &self,
        image_path: &Path,
    ) -> Result<PortfolioImageRecognition, AiError> {
        self.run_image_json(
            image_path.to_path_buf(),
            portfolio_image_recognition_prompt(),
        )
        .await
    }
}

async fn run_cli_json<T, B>(
    backend: &B,
    settings: &CliSettings,
    prompt: String,
) -> Result<T, AiError>
where
    T: DeserializeOwned,
    B: CliBackend,
{
    let command_spec = backend.build_command(settings, prompt);
    run_cli_command_json(backend, settings, command_spec).await
}

async fn run_cli_image_json<T, B>(
    backend: &B,
    settings: &CliSettings,
    image_path: &Path,
    prompt: String,
) -> Result<T, AiError>
where
    T: DeserializeOwned,
    B: CliBackend,
{
    let schema_file = TemporaryCliFile::write(
        "prudentia-portfolio-image-schema",
        "json",
        portfolio_image_recognition_schema().as_bytes(),
    )?;
    let command_spec =
        backend.build_image_command(settings, image_path, Some(schema_file.path()), prompt);
    run_cli_command_json(backend, settings, command_spec).await
}

async fn run_cli_command_json<T, B>(
    backend: &B,
    settings: &CliSettings,
    mut command_spec: CliCommand,
) -> Result<T, AiError>
where
    T: DeserializeOwned,
    B: CliBackend,
{
    let started_at = Instant::now();
    let invocation_id = Uuid::new_v4();
    let output_last_message = if backend.kind() == CliProviderKind::Codex {
        let file = TemporaryCliFile::write("prudentia-cli-last-message", "json", b"")?;
        add_codex_json_capture(&mut command_spec, file.path());
        Some(file)
    } else {
        None
    };
    let has_image = command_spec.args.iter().any(|arg| arg == "--image");
    let has_schema = command_spec.args.iter().any(|arg| arg == "--output-schema");
    let captures_json_events = command_spec.args.iter().any(|arg| arg == "--json");
    let arg_count = command_spec.args.len();
    tracing::info!(
        invocation_id = %invocation_id,
        provider = backend.kind().as_str(),
        program = %command_spec.program,
        model = settings.model.as_deref().unwrap_or("default"),
        profile = settings.profile.as_deref().unwrap_or("default"),
        arg_count,
        has_image,
        has_schema,
        captures_json_events,
        "AI CLI invocation scheduled"
    );
    tracing::info!(
        invocation_id = %invocation_id,
        provider = backend.kind().as_str(),
        program = %command_spec.program,
        arg_count,
        has_image,
        has_schema,
        "AI CLI command started"
    );
    let output = Command::new(&command_spec.program)
        .args(&command_spec.args)
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|err| {
            tracing::warn!(
                invocation_id = %invocation_id,
                provider = backend.kind().as_str(),
                program = %command_spec.program,
                elapsed_ms = started_at.elapsed().as_millis(),
                error = %err,
                "AI CLI command failed to start"
            );
            AiError::Provider(format!(
                "failed to run {} CLI. {} Error: {err}",
                backend.kind().as_str(),
                backend.auth_hint(settings)
            ))
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        log_cli_token_usage(
            invocation_id,
            backend.kind(),
            command_spec.program.as_str(),
            stdout.as_ref(),
        );
        tracing::warn!(
            invocation_id = %invocation_id,
            provider = backend.kind().as_str(),
            program = %command_spec.program,
            elapsed_ms = started_at.elapsed().as_millis(),
            status = output.status.code().unwrap_or_default(),
            stderr_bytes = output.stderr.len(),
            stdout_bytes = output.stdout.len(),
            "AI CLI command exited with failure"
        );
        return Err(AiError::Provider(format!(
            "{} CLI failed. {} stderr: {stderr}",
            backend.kind().as_str(),
            backend.auth_hint(settings)
        )));
    }
    log_cli_token_usage(
        invocation_id,
        backend.kind(),
        command_spec.program.as_str(),
        stdout.as_ref(),
    );
    tracing::info!(
        invocation_id = %invocation_id,
        provider = backend.kind().as_str(),
        program = %command_spec.program,
        elapsed_ms = started_at.elapsed().as_millis(),
        status = output.status.code().unwrap_or_default(),
        stdout_bytes = output.stdout.len(),
        stderr_bytes = output.stderr.len(),
        "AI CLI command completed"
    );

    let response_text = cli_response_text(stdout.as_ref(), output_last_message.as_ref());
    let json = extract_json_object(response_text.trim()).ok_or_else(|| {
        tracing::warn!(
            invocation_id = %invocation_id,
            provider = backend.kind().as_str(),
            program = %command_spec.program,
            stdout_bytes = output.stdout.len(),
            "AI CLI command returned no JSON object"
        );
        AiError::Provider(format!(
            "{} CLI did not return a JSON object",
            backend.kind().as_str()
        ))
    })?;

    serde_json::from_str(json).map_err(|err| {
        tracing::warn!(
            invocation_id = %invocation_id,
            provider = backend.kind().as_str(),
            program = %command_spec.program,
            error = %err,
            "AI CLI JSON parse failed"
        );
        AiError::Provider(format!(
            "failed to parse {} CLI JSON response: {err}. response: {json}",
            backend.kind().as_str()
        ))
    })
}

fn add_codex_json_capture(command: &mut CliCommand, output_last_message_path: &Path) {
    let insert_at = command.args.len().saturating_sub(1);
    command.args.insert(insert_at, "--json".to_string());
    command.args.insert(insert_at + 1, "-o".to_string());
    command.args.insert(
        insert_at + 2,
        output_last_message_path.display().to_string(),
    );
}

fn cli_response_text(stdout: &str, output_last_message: Option<&TemporaryCliFile>) -> String {
    if let Some(file) = output_last_message {
        let response = fs::read_to_string(file.path()).unwrap_or_default();
        if !response.trim().is_empty() {
            return response;
        }
    }

    codex_last_agent_message(stdout).unwrap_or_else(|| stdout.to_string())
}

fn codex_last_agent_message(output: &str) -> Option<String> {
    let mut last_message = None;
    for line in output.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
            continue;
        };

        if value.get("type").and_then(|field| field.as_str()) != Some("item.completed") {
            continue;
        }

        let Some(item) = value.get("item") else {
            continue;
        };
        if item.get("type").and_then(|field| field.as_str()) != Some("agent_message") {
            continue;
        }
        if let Some(text) = item.get("text").and_then(|field| field.as_str()) {
            last_message = Some(text.to_string());
        }
    }
    last_message
}

fn log_cli_token_usage(
    invocation_id: Uuid,
    provider: CliProviderKind,
    program: &str,
    output: &str,
) {
    let json_event_count = output
        .lines()
        .filter(|line| serde_json::from_str::<serde_json::Value>(line.trim()).is_ok())
        .count();

    if let Some(usage) = parse_cli_token_usage(output) {
        tracing::info!(
            invocation_id = %invocation_id,
            provider = provider.as_str(),
            program,
            token_usage_available = true,
            input_tokens = usage.input_tokens.unwrap_or_default(),
            cached_input_tokens = usage.cached_input_tokens.unwrap_or_default(),
            output_tokens = usage.output_tokens.unwrap_or_default(),
            reasoning_output_tokens = usage.reasoning_output_tokens.unwrap_or_default(),
            total_tokens = usage.total_tokens.unwrap_or_default(),
            json_event_count,
            "AI CLI token usage"
        );
    } else {
        tracing::info!(
            invocation_id = %invocation_id,
            provider = provider.as_str(),
            program,
            token_usage_available = false,
            json_event_count,
            "AI CLI token usage unavailable"
        );
    }
}

fn parse_cli_token_usage(output: &str) -> Option<CliTokenUsage> {
    let mut usage = None;

    for line in output.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
            continue;
        };

        if value.get("type").and_then(|field| field.as_str()) == Some("turn.completed") {
            if let Some(parsed) = value.get("usage").and_then(CliTokenUsage::from_value) {
                usage = Some(parsed);
            }
            continue;
        }

        if value.get("type").and_then(|field| field.as_str()) == Some("event_msg") {
            let Some(payload) = value.get("payload") else {
                continue;
            };
            if payload.get("type").and_then(|field| field.as_str()) != Some("token_count") {
                continue;
            }
            let parsed = payload
                .get("info")
                .and_then(|info| info.get("last_token_usage"))
                .and_then(CliTokenUsage::from_value)
                .or_else(|| {
                    payload
                        .get("info")
                        .and_then(|info| info.get("total_token_usage"))
                        .and_then(CliTokenUsage::from_value)
                });
            if let Some(parsed) = parsed {
                usage = Some(parsed);
            }
        }
    }

    usage
}

impl CliTokenUsage {
    fn from_value(value: &serde_json::Value) -> Option<Self> {
        let input_tokens = number_field(value, &["input_tokens", "prompt_tokens"]);
        let cached_input_tokens =
            number_field(value, &["cached_input_tokens", "cached_prompt_tokens"]);
        let output_tokens = number_field(value, &["output_tokens", "completion_tokens"]);
        let reasoning_output_tokens =
            number_field(value, &["reasoning_output_tokens", "reasoning_tokens"]);
        let total_tokens = number_field(value, &["total_tokens"]).or_else(|| {
            input_tokens
                .zip(output_tokens)
                .map(|(input, output)| input + output)
        });

        let usage = Self {
            input_tokens,
            cached_input_tokens,
            output_tokens,
            reasoning_output_tokens,
            total_tokens,
        };

        if usage.input_tokens.is_some()
            || usage.cached_input_tokens.is_some()
            || usage.output_tokens.is_some()
            || usage.reasoning_output_tokens.is_some()
            || usage.total_tokens.is_some()
        {
            Some(usage)
        } else {
            None
        }
    }
}

fn number_field(value: &serde_json::Value, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(|field| field.as_u64()))
}

struct TemporaryCliFile {
    path: PathBuf,
}

impl TemporaryCliFile {
    fn write(prefix: &str, extension: &str, bytes: &[u8]) -> Result<Self, AiError> {
        let file_name = format!("{prefix}-{}.{}", Uuid::new_v4(), extension);
        let path = std::env::temp_dir().join(file_name);
        fs::write(&path, bytes)
            .map_err(|err| AiError::Provider(format!("failed to write temporary file: {err}")))?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryCliFile {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_file(&self.path) {
            tracing::debug!(path = %self.path.display(), error = %error, "temporary CLI file cleanup failed");
        }
    }
}

pub mod codex;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_codex_json_usage_from_turn_completed_event() {
        let output = r#"{"type":"thread.started","thread_id":"thread-1"}
{"type":"turn.completed","usage":{"input_tokens":14785,"cached_input_tokens":5504,"output_tokens":427,"reasoning_output_tokens":416}}"#;

        let usage = parse_cli_token_usage(output).expect("usage should be parsed");

        assert_eq!(usage.input_tokens, Some(14785));
        assert_eq!(usage.cached_input_tokens, Some(5504));
        assert_eq!(usage.output_tokens, Some(427));
        assert_eq!(usage.reasoning_output_tokens, Some(416));
        assert_eq!(usage.total_tokens, Some(15212));
    }

    #[test]
    fn missing_cli_usage_returns_none() {
        let output = r#"{"type":"thread.started","thread_id":"thread-1"}"#;

        assert_eq!(parse_cli_token_usage(output), None);
    }

    #[test]
    fn codex_json_capture_options_are_inserted_before_prompt() {
        let mut command = CliCommand {
            program: "codex".to_string(),
            args: vec![
                "exec".to_string(),
                "--skip-git-repo-check".to_string(),
                "return json".to_string(),
            ],
        };

        add_codex_json_capture(&mut command, Path::new("/tmp/prudentia-last-message.json"));

        assert_eq!(
            command.args,
            vec![
                "exec",
                "--skip-git-repo-check",
                "--json",
                "-o",
                "/tmp/prudentia-last-message.json",
                "return json"
            ]
        );
    }
}
