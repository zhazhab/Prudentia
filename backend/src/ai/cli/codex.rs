use std::path::Path;

use crate::ai::cli::{CliBackend, CliCommand, CliProviderKind, CliSettings};

#[derive(Debug, Clone, Copy)]
pub struct CodexCliBackend;

impl CliBackend for CodexCliBackend {
    fn kind(&self) -> CliProviderKind {
        CliProviderKind::Codex
    }

    fn build_command(&self, settings: &CliSettings, prompt: String) -> CliCommand {
        let mut args = base_exec_args(settings);

        args.push(prompt);

        CliCommand {
            program: settings.path.clone(),
            args,
        }
    }

    fn build_json_command(
        &self,
        settings: &CliSettings,
        schema_path: Option<&Path>,
        prompt: String,
    ) -> CliCommand {
        let mut command = self.build_command(settings, prompt);
        if let Some(schema_path) = schema_path {
            let insert_at = command.args.len().saturating_sub(1);
            command
                .args
                .insert(insert_at, "--output-schema".to_string());
            command
                .args
                .insert(insert_at + 1, schema_path.display().to_string());
        }
        command
    }

    fn build_image_command(
        &self,
        settings: &CliSettings,
        image_path: &Path,
        schema_path: Option<&Path>,
        prompt: String,
    ) -> CliCommand {
        let mut args = base_exec_args(settings);
        args.push("--image".to_string());
        args.push(image_path.display().to_string());

        if let Some(schema_path) = schema_path {
            args.push("--output-schema".to_string());
            args.push(schema_path.display().to_string());
        }

        args.push(prompt);

        CliCommand {
            program: settings.path.clone(),
            args,
        }
    }

    fn auth_hint(&self, settings: &CliSettings) -> String {
        format!(
            "Install Codex and authenticate with `{}` first.",
            self.kind()
                .login_command(settings)
                .unwrap_or_else(|| "codex login --device-auth".to_string())
        )
    }
}

fn base_exec_args(settings: &CliSettings) -> Vec<String> {
    let mut args = vec![
        "exec".to_string(),
        "--skip-git-repo-check".to_string(),
        "--sandbox".to_string(),
        "read-only".to_string(),
        "-c".to_string(),
        "approval_policy=never".to_string(),
        "--ephemeral".to_string(),
        "--ignore-user-config".to_string(),
        "--ignore-rules".to_string(),
    ];

    for feature in [
        "apps",
        "plugins",
        "browser_use",
        "browser_use_external",
        "computer_use",
        "in_app_browser",
        "image_generation",
        "multi_agent",
        "shell_tool",
        "unified_exec",
        "workspace_dependencies",
    ] {
        args.push("--disable".to_string());
        args.push(feature.to_string());
    }
    args.push("-C".to_string());
    args.push(std::env::temp_dir().display().to_string());

    if let Some(model) = settings
        .model
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        args.push("--model".to_string());
        args.push(model.to_string());
    }

    if let Some(profile) = settings
        .profile
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        args.push("--profile".to_string());
        args.push(profile.to_string());
    }

    args
}

pub(super) fn codex_provider_stage(line: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(line).ok()?;
    let event_type = value.get("type")?.as_str()?;
    let stage = match event_type {
        "thread.started" => "provider_ready",
        "turn.started" => "provider_reading_context",
        "item.started" | "item.completed" => codex_item_stage(&value, event_type)?,
        "turn.completed" => "provider_completed",
        "turn.failed" => "provider_failed",
        _ => return None,
    };
    Some(stage.to_string())
}

fn codex_item_stage(value: &serde_json::Value, event_type: &str) -> Option<&'static str> {
    let item_type = value.get("item")?.get("type")?.as_str()?;
    match item_type {
        "reasoning" => Some("provider_analyzing_evidence"),
        "agent_message" => Some("provider_writing_response"),
        "command_execution" | "mcp_tool_call" | "web_search" | "computer_use" => {
            if event_type == "item.started" {
                Some("provider_using_tool")
            } else {
                Some("provider_analyzing_evidence")
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_json_items_report_meaningful_provider_stages() {
        assert_eq!(
            codex_provider_stage(r#"{"type":"turn.started"}"#).as_deref(),
            Some("provider_reading_context")
        );
        assert_eq!(
            codex_provider_stage(
                r#"{"type":"item.completed","item":{"type":"reasoning","text":"checking evidence"}}"#
            )
            .as_deref(),
            Some("provider_analyzing_evidence")
        );
        assert_eq!(
            codex_provider_stage(
                r#"{"type":"item.started","item":{"type":"mcp_tool_call","name":"search"}}"#
            )
            .as_deref(),
            Some("provider_using_tool")
        );
        assert_eq!(
            codex_provider_stage(
                r#"{"type":"item.completed","item":{"type":"agent_message","text":"answer"}}"#
            )
            .as_deref(),
            Some("provider_writing_response")
        );
    }

    #[test]
    fn builds_codex_exec_command_with_model_and_profile() {
        let backend = CodexCliBackend;
        let command = backend.build_command(
            &CliSettings {
                provider: CliProviderKind::Codex,
                path: "codex".to_string(),
                model: Some("gpt-5.4".to_string()),
                profile: Some("personal".to_string()),
            },
            "return json".to_string(),
        );

        for required in [
            "--ephemeral",
            "--ignore-user-config",
            "--ignore-rules",
            "apps",
            "plugins",
            "browser_use",
            "browser_use_external",
            "computer_use",
            "in_app_browser",
            "image_generation",
            "multi_agent",
            "shell_tool",
            "unified_exec",
            "workspace_dependencies",
        ] {
            assert!(
                command.args.iter().any(|argument| argument == required),
                "missing isolated conversation argument: {required}"
            );
        }
        assert!(command.args.iter().any(|argument| argument == "-C"));

        assert_eq!(command.program, "codex");
        assert_eq!(option_value(&command.args, "--model"), Some("gpt-5.4"));
        assert_eq!(option_value(&command.args, "--profile"), Some("personal"));
        assert_eq!(
            option_value(&command.args, "-C"),
            Some(std::env::temp_dir().to_string_lossy().as_ref())
        );
        assert_eq!(command.args.last().map(String::as_str), Some("return json"));
    }

    #[test]
    fn builds_codex_exec_command_with_image_and_schema() {
        let backend = CodexCliBackend;
        let command = backend.build_image_command(
            &CliSettings {
                provider: CliProviderKind::Codex,
                path: "codex".to_string(),
                model: Some("gpt-5.4".to_string()),
                profile: Some("personal".to_string()),
            },
            std::path::Path::new("/tmp/positions.png"),
            Some(std::path::Path::new("/tmp/schema.json")),
            "recognize portfolio image".to_string(),
        );

        assert!(command
            .args
            .iter()
            .any(|argument| argument == "--ephemeral"));
        assert!(command
            .args
            .iter()
            .any(|argument| argument == "--ignore-user-config"));

        assert_eq!(command.program, "codex");
        assert_eq!(
            option_value(&command.args, "--image"),
            Some("/tmp/positions.png")
        );
        assert_eq!(
            option_value(&command.args, "--output-schema"),
            Some("/tmp/schema.json")
        );
        assert_eq!(
            command.args.last().map(String::as_str),
            Some("recognize portfolio image")
        );
    }

    #[test]
    fn builds_codex_json_command_with_schema_before_prompt() {
        let backend = CodexCliBackend;
        let command = backend.build_json_command(
            &CliSettings::default(),
            Some(std::path::Path::new("/tmp/schema.json")),
            "project conversation".to_string(),
        );

        assert_eq!(
            option_value(&command.args, "--output-schema"),
            Some("/tmp/schema.json")
        );
        assert_eq!(
            command.args.last().map(String::as_str),
            Some("project conversation")
        );
    }

    fn option_value<'a>(args: &'a [String], option: &str) -> Option<&'a str> {
        args.iter()
            .position(|argument| argument == option)
            .and_then(|index| args.get(index + 1))
            .map(String::as_str)
    }
}
