use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub bind_addr: String,
    pub database_url: String,
    pub ai_provider: String,
    pub openai_api_key: Option<String>,
    pub openai_base_url: String,
    pub openai_model: String,
    pub openai_model_simple: String,
    pub openai_model_standard: String,
    pub openai_model_deep: String,
    pub ai_cli_provider: String,
    pub ai_cli_path: String,
    pub ai_cli_model: Option<String>,
    pub ai_cli_model_simple: String,
    pub ai_cli_model_standard: String,
    pub ai_cli_model_deep: String,
    pub ai_cli_profile: Option<String>,
    pub market_data_provider: String,
    pub alpha_vantage_api_key: Option<String>,
    pub price_refresh_interval_secs: Duration,
    pub price_refresh_ttl_secs: Duration,
    pub symbol_directory_provider: String,
    pub symbol_directory_refresh_interval_secs: Duration,
    pub web_research_provider: WebResearchProviderKind,
    pub tavily_api_key: Option<String>,
    pub workspace_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebResearchProviderKind {
    PublicSources,
    Tavily,
    Disabled,
    Invalid(String),
}

impl WebResearchProviderKind {
    fn parse(value: String) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "public" | "public_sources" => Self::PublicSources,
            "tavily" => Self::Tavily,
            "disabled" => Self::Disabled,
            _ => Self::Invalid(value),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{local_state_dir_from_git_common_dir, resolve_sqlite_url, WebResearchProviderKind};
    use std::path::Path;

    #[test]
    fn relative_sqlite_url_resolves_under_original_repo_dir() {
        let resolved = resolve_sqlite_url(
            Some("sqlite://data/prudentia.sqlite".to_string()),
            Path::new("/repo"),
        );

        assert_eq!(resolved, "sqlite:///repo/data/prudentia.sqlite");
    }

    #[test]
    fn git_common_dir_parent_is_the_default_local_state_dir() {
        assert_eq!(
            local_state_dir_from_git_common_dir(Path::new("/repo/.git")),
            Some(Path::new("/repo").to_path_buf())
        );
    }

    #[test]
    fn research_provider_configuration_is_typed_at_the_environment_edge() {
        assert_eq!(
            WebResearchProviderKind::parse("public_sources".to_string()),
            WebResearchProviderKind::PublicSources
        );
        assert_eq!(
            WebResearchProviderKind::parse("disabled".to_string()),
            WebResearchProviderKind::Disabled
        );
        assert_eq!(
            WebResearchProviderKind::parse("typo".to_string()),
            WebResearchProviderKind::Invalid("typo".to_string())
        );
    }
}

impl AppConfig {
    pub fn from_env() -> Self {
        Self::from_env_with_paths(&LocalAppPaths::discover())
    }

    pub fn from_env_with_paths(paths: &LocalAppPaths) -> Self {
        Self {
            bind_addr: env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string()),
            database_url: resolve_sqlite_url(env::var("DATABASE_URL").ok(), &paths.root_dir),
            ai_provider: env::var("AI_PROVIDER").unwrap_or_else(|_| "cli".to_string()),
            openai_api_key: env::var("OPENAI_API_KEY").ok(),
            openai_base_url: env::var("OPENAI_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_string()),
            openai_model: env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4.1-mini".to_string()),
            openai_model_simple: routed_openai_model("OPENAI_MODEL_SIMPLE"),
            openai_model_standard: routed_openai_model("OPENAI_MODEL_STANDARD"),
            openai_model_deep: routed_openai_model("OPENAI_MODEL_DEEP"),
            ai_cli_provider: env::var("AI_CLI_PROVIDER").unwrap_or_else(|_| "codex".to_string()),
            ai_cli_path: env::var("AI_CLI_PATH")
                .or_else(|_| env::var("CODEX_CLI_PATH"))
                .unwrap_or_else(|_| "codex".to_string()),
            ai_cli_model: env::var("AI_CLI_MODEL")
                .or_else(|_| env::var("CODEX_MODEL"))
                .ok()
                .filter(|value| !value.is_empty()),
            ai_cli_model_simple: routed_cli_model("AI_CLI_MODEL_SIMPLE", "gpt-5.6-luna"),
            ai_cli_model_standard: routed_cli_model("AI_CLI_MODEL_STANDARD", "gpt-5.6-terra"),
            ai_cli_model_deep: routed_cli_model("AI_CLI_MODEL_DEEP", "gpt-5.6-sol"),
            ai_cli_profile: env::var("AI_CLI_PROFILE")
                .or_else(|_| env::var("CODEX_PROFILE"))
                .ok()
                .filter(|value| !value.is_empty()),
            market_data_provider: env::var("MARKET_DATA_PROVIDER")
                .unwrap_or_else(|_| "mock".to_string()),
            alpha_vantage_api_key: env::var("ALPHA_VANTAGE_API_KEY").ok(),
            price_refresh_interval_secs: env::var("PRICE_REFRESH_INTERVAL_SECS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .map(Duration::from_secs)
                .unwrap_or_else(|| Duration::from_secs(60 * 60)),
            price_refresh_ttl_secs: env::var("PRICE_REFRESH_TTL_SECS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|seconds| *seconds > 0)
                .map(Duration::from_secs)
                .unwrap_or_else(|| Duration::from_secs(24 * 60 * 60)),
            symbol_directory_provider: env::var("SYMBOL_DIRECTORY_PROVIDER")
                .unwrap_or_else(|_| "public".to_string()),
            symbol_directory_refresh_interval_secs: env::var(
                "SYMBOL_DIRECTORY_REFRESH_INTERVAL_SECS",
            )
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|seconds| *seconds > 0)
            .map(Duration::from_secs)
            .unwrap_or_else(|| Duration::from_secs(24 * 60 * 60)),
            web_research_provider: WebResearchProviderKind::parse(
                env::var("WEB_RESEARCH_PROVIDER").unwrap_or_else(|_| "public_sources".to_string()),
            ),
            tavily_api_key: env::var("TAVILY_API_KEY").ok(),
            workspace_dir: paths.root_dir.join("data/workspace"),
        }
    }
}

fn routed_openai_model(key: &str) -> String {
    env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            env::var("OPENAI_MODEL")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_else(|| "gpt-4.1-mini".to_string())
}

fn routed_cli_model(key: &str, default: &str) -> String {
    env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            env::var("AI_CLI_MODEL")
                .or_else(|_| env::var("CODEX_MODEL"))
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_else(|| default.to_string())
}

#[derive(Debug, Clone)]
pub struct LocalAppPaths {
    pub root_dir: PathBuf,
    pub env_path: PathBuf,
}

impl LocalAppPaths {
    pub fn discover() -> Self {
        let root_dir = env::var("PRUDENTIA_LOCAL_DIR")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
            .map(resolve_against_current_dir)
            .or_else(|| {
                git_common_dir().and_then(|path| local_state_dir_from_git_common_dir(&path))
            })
            .unwrap_or_else(|| PathBuf::from("."));
        let env_path = root_dir.join(".env");
        Self { root_dir, env_path }
    }

    pub fn load_env(&self) {
        if self.env_path.exists() {
            dotenvy::from_path(&self.env_path).ok();
        } else {
            dotenvy::dotenv().ok();
        }
    }
}

pub fn resolve_sqlite_url(value: Option<String>, local_state_dir: &Path) -> String {
    let value = value.unwrap_or_else(|| "sqlite://data/prudentia.sqlite".to_string());
    let Some(path) = value.strip_prefix("sqlite://") else {
        return value;
    };
    if path == ":memory:" || path.starts_with('/') || path.is_empty() {
        return value;
    }
    format!("sqlite://{}", local_state_dir.join(path).to_string_lossy())
}

fn resolve_against_current_dir(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        return path;
    }
    env::current_dir()
        .map(|current_dir| current_dir.join(&path))
        .unwrap_or(path)
}

fn git_common_dir() -> Option<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--git-common-dir"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8(output.stdout).ok()?;
    let path = PathBuf::from(path.trim());
    Some(resolve_against_current_dir(path))
}

fn local_state_dir_from_git_common_dir(git_common_dir: &Path) -> Option<PathBuf> {
    if git_common_dir
        .file_name()
        .is_some_and(|name| name == ".git")
    {
        return git_common_dir.parent().map(Path::to_path_buf);
    }
    None
}
