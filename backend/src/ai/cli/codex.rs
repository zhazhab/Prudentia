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
        "--ask-for-approval".to_string(),
        "never".to_string(),
    ];

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

#[cfg(test)]
mod tests {
    use super::*;

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

        assert_eq!(command.program, "codex");
        assert_eq!(
            command.args,
            vec![
                "exec",
                "--skip-git-repo-check",
                "--sandbox",
                "read-only",
                "--ask-for-approval",
                "never",
                "--model",
                "gpt-5.4",
                "--profile",
                "personal",
                "return json"
            ]
        );
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

        assert_eq!(command.program, "codex");
        assert_eq!(
            command.args,
            vec![
                "exec",
                "--skip-git-repo-check",
                "--sandbox",
                "read-only",
                "--ask-for-approval",
                "never",
                "--model",
                "gpt-5.4",
                "--profile",
                "personal",
                "--image",
                "/tmp/positions.png",
                "--output-schema",
                "/tmp/schema.json",
                "recognize portfolio image"
            ]
        );
    }
}
