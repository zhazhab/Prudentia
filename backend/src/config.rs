use std::{env, time::Duration};

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub bind_addr: String,
    pub database_url: String,
    pub ai_provider: String,
    pub openai_api_key: Option<String>,
    pub openai_base_url: String,
    pub openai_model: String,
    pub ai_cli_provider: String,
    pub ai_cli_path: String,
    pub ai_cli_model: Option<String>,
    pub ai_cli_profile: Option<String>,
    pub market_data_provider: String,
    pub alpha_vantage_api_key: Option<String>,
    pub price_refresh_interval_secs: Duration,
    pub price_refresh_ttl_secs: Duration,
    pub symbol_directory_provider: String,
    pub symbol_directory_refresh_interval_secs: Duration,
}

impl AppConfig {
    pub fn from_env() -> Self {
        Self {
            bind_addr: env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string()),
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite://data/prudentia.sqlite".to_string()),
            ai_provider: env::var("AI_PROVIDER").unwrap_or_else(|_| "mock".to_string()),
            openai_api_key: env::var("OPENAI_API_KEY").ok(),
            openai_base_url: env::var("OPENAI_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_string()),
            openai_model: env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4.1-mini".to_string()),
            ai_cli_provider: env::var("AI_CLI_PROVIDER").unwrap_or_else(|_| "codex".to_string()),
            ai_cli_path: env::var("AI_CLI_PATH")
                .or_else(|_| env::var("CODEX_CLI_PATH"))
                .unwrap_or_else(|_| "codex".to_string()),
            ai_cli_model: env::var("AI_CLI_MODEL")
                .or_else(|_| env::var("CODEX_MODEL"))
                .ok()
                .filter(|value| !value.is_empty()),
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
        }
    }
}
