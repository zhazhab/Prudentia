use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    time::Instant,
};

use async_trait::async_trait;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    process::Command,
    sync::mpsc,
};
use uuid::Uuid;

pub mod codex;
mod projection;
mod usage;
use codex::codex_provider_stage;
use projection::CliConversationProjection;
use usage::log_cli_token_usage;

use crate::{
    ai::{
        prompt::{
            conversation_projection_cli_prompt, conversation_projection_schema,
            conversation_response_prompt, investment_system_refinement_prompt, memo_chat_prompt,
            memo_extraction_prompt, parse_json_object, portfolio_image_recognition_prompt,
            portfolio_image_recognition_schema, portfolio_review_prompt,
            research_distillation_prompt, stock_snapshot_prompt,
        },
        AiError, AiProvider, AiProviderEvent, ConversationContext, ConversationProjection,
        InvestmentSystemRefinement, MemoChatContext, MemoExtraction, PortfolioImageRecognition,
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
    fn build_json_command(
        &self,
        settings: &CliSettings,
        schema_path: Option<&Path>,
        prompt: String,
    ) -> CliCommand;
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

    async fn run_text(&self, prompt: String) -> Result<String, AiError> {
        run_cli_text(&self.backend, &self.settings, prompt).await
    }

    async fn run_json<T>(&self, prompt: String) -> Result<T, AiError>
    where
        T: DeserializeOwned + Send + 'static,
    {
        run_cli_json(&self.backend, &self.settings, prompt).await
    }

    async fn run_json_with_schema<T>(&self, prompt: String, schema: &str) -> Result<T, AiError>
    where
        T: DeserializeOwned + Send + 'static,
    {
        run_cli_json_with_schema(&self.backend, &self.settings, prompt, schema).await
    }

    async fn run_image_json<T>(&self, image_path: PathBuf, prompt: String) -> Result<T, AiError>
    where
        T: DeserializeOwned + Send + 'static,
    {
        run_cli_image_json(&self.backend, &self.settings, &image_path, prompt).await
    }

    async fn run_conversation(
        &self,
        context: &ConversationContext,
        prompt: String,
        events: mpsc::UnboundedSender<AiProviderEvent>,
    ) -> Result<String, AiError> {
        let mut command = self.backend.build_command(&self.settings, prompt);
        let image_paths = context
            .attachments
            .iter()
            .filter(|attachment| attachment.mime_type.starts_with("image/"))
            .filter_map(|attachment| attachment.local_path.as_deref())
            .collect::<Vec<_>>();
        add_image_arguments(&mut command, &image_paths);
        run_cli_command_text_stream(&self.backend, &self.settings, command, events).await
    }
}

#[async_trait]
impl<B> AiProvider for CliAiProvider<B>
where
    B: CliBackend,
{
    async fn respond_to_conversation(
        &self,
        context: &ConversationContext,
        locale: Locale,
        events: mpsc::UnboundedSender<AiProviderEvent>,
    ) -> Result<String, AiError> {
        self.run_conversation(
            context,
            conversation_response_prompt(context, locale),
            events,
        )
        .await
    }

    async fn project_conversation(
        &self,
        context: &ConversationContext,
        assistant_response: &str,
        locale: Locale,
    ) -> Result<ConversationProjection, AiError> {
        let projection: CliConversationProjection = self
            .run_json_with_schema(
                conversation_projection_cli_prompt(context, assistant_response, locale),
                conversation_projection_schema(),
            )
            .await?;
        projection.try_into()
    }

    async fn respond_to_memo_chat(
        &self,
        context: &MemoChatContext,
        locale: Locale,
    ) -> Result<String, AiError> {
        self.run_text(memo_chat_prompt(context, locale)).await
    }

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

async fn run_cli_json_with_schema<T, B>(
    backend: &B,
    settings: &CliSettings,
    prompt: String,
    schema: &str,
) -> Result<T, AiError>
where
    T: DeserializeOwned,
    B: CliBackend,
{
    let schema_file = TemporaryCliFile::write(
        "prudentia-conversation-projection-schema",
        "json",
        schema.as_bytes(),
    )?;
    let command_spec = backend.build_json_command(settings, Some(schema_file.path()), prompt);
    run_cli_command_json(backend, settings, command_spec).await
}

async fn run_cli_text<B>(
    backend: &B,
    settings: &CliSettings,
    prompt: String,
) -> Result<String, AiError>
where
    B: CliBackend,
{
    let command_spec = backend.build_command(settings, prompt);
    run_cli_command_text(backend, settings, command_spec).await
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
    parse_json_object(response_text.trim()).map_err(|err| {
        tracing::warn!(
            invocation_id = %invocation_id,
            provider = backend.kind().as_str(),
            program = %command_spec.program,
            error = %err,
            "AI CLI JSON parse failed"
        );
        AiError::Provider(format!(
            "failed to parse {} CLI JSON response: {err}",
            backend.kind().as_str()
        ))
    })
}

async fn run_cli_command_text<B>(
    backend: &B,
    settings: &CliSettings,
    mut command_spec: CliCommand,
) -> Result<String, AiError>
where
    B: CliBackend,
{
    let started_at = Instant::now();
    let invocation_id = Uuid::new_v4();
    let output_last_message = if backend.kind() == CliProviderKind::Codex {
        let file = TemporaryCliFile::write("prudentia-cli-last-message", "txt", b"")?;
        add_codex_json_capture(&mut command_spec, file.path());
        Some(file)
    } else {
        None
    };
    let arg_count = command_spec.args.len();
    tracing::info!(
        invocation_id = %invocation_id,
        provider = backend.kind().as_str(),
        program = %command_spec.program,
        model = settings.model.as_deref().unwrap_or("default"),
        profile = settings.profile.as_deref().unwrap_or("default"),
        arg_count,
        captures_json_events = command_spec.args.iter().any(|arg| arg == "--json"),
        "AI CLI text invocation scheduled"
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
                "AI CLI text command failed to start"
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
            "AI CLI text command exited with failure"
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
        "AI CLI text command completed"
    );

    let response_text = cli_response_text(stdout.as_ref(), output_last_message.as_ref())
        .trim()
        .to_string();
    if response_text.is_empty() {
        return Err(AiError::Provider(format!(
            "{} CLI returned an empty response",
            backend.kind().as_str()
        )));
    }

    Ok(response_text)
}

async fn run_cli_command_text_stream<B>(
    backend: &B,
    settings: &CliSettings,
    mut command_spec: CliCommand,
    events: mpsc::UnboundedSender<AiProviderEvent>,
) -> Result<String, AiError>
where
    B: CliBackend,
{
    let started_at = Instant::now();
    let invocation_id = Uuid::new_v4();
    let output_last_message = if backend.kind() == CliProviderKind::Codex {
        let file = TemporaryCliFile::write("prudentia-cli-last-message", "txt", b"")?;
        add_codex_json_capture(&mut command_spec, file.path());
        Some(file)
    } else {
        None
    };
    let _ = events.send(AiProviderEvent::Stage {
        provider: backend.kind().as_str().to_string(),
        stage: "process_starting".to_string(),
    });
    let mut child = Command::new(&command_spec.program)
        .args(&command_spec.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|err| {
            AiError::Provider(format!(
                "failed to run {} CLI. {} Error: {err}",
                backend.kind().as_str(),
                backend.auth_hint(settings)
            ))
        })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AiError::Provider("AI CLI stdout was unavailable".to_string()))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| AiError::Provider("AI CLI stderr was unavailable".to_string()))?;
    let stderr_task = tokio::spawn(async move {
        let mut output = String::new();
        let _ = stderr.read_to_string(&mut output).await;
        output
    });
    let mut lines = BufReader::new(stdout).lines();
    let mut stdout_text = String::new();
    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|err| AiError::Provider(format!("failed to read AI CLI output: {err}")))?
    {
        stdout_text.push_str(&line);
        stdout_text.push('\n');
        if let Some(stage) = codex_provider_stage(&line) {
            let _ = events.send(AiProviderEvent::Stage {
                provider: backend.kind().as_str().to_string(),
                stage,
            });
        }
    }
    let status = child
        .wait()
        .await
        .map_err(|err| AiError::Provider(format!("failed to wait for AI CLI: {err}")))?;
    let stderr_text = stderr_task.await.unwrap_or_default();
    if !status.success() {
        return Err(AiError::Provider(format!(
            "{} CLI failed. {} stderr: {}",
            backend.kind().as_str(),
            backend.auth_hint(settings),
            stderr_text.trim()
        )));
    }
    log_cli_token_usage(
        invocation_id,
        backend.kind(),
        command_spec.program.as_str(),
        &stdout_text,
    );
    tracing::info!(
        invocation_id = %invocation_id,
        provider = backend.kind().as_str(),
        elapsed_ms = started_at.elapsed().as_millis(),
        "AI CLI streamed command completed"
    );
    let response = cli_response_text(&stdout_text, output_last_message.as_ref())
        .trim()
        .to_string();
    if response.is_empty() {
        return Err(AiError::Provider(format!(
            "{} CLI returned an empty response",
            backend.kind().as_str()
        )));
    }
    Ok(response)
}

fn add_image_arguments(command: &mut CliCommand, image_paths: &[&str]) {
    let insert_at = command.args.len().saturating_sub(1);
    for (offset, path) in image_paths.iter().enumerate() {
        let index = insert_at + offset * 2;
        command.args.insert(index, "--image".to_string());
        command.args.insert(index + 1, (*path).to_string());
    }
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

#[cfg(test)]
mod tests {
    use super::*;

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
