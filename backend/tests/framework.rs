use std::sync::Arc;

use async_trait::async_trait;
use prudentia_backend::{
    ai::{
        cli::{CliProviderKind, CliSettings},
        runtime::{AiProviderKind, AiRuntime, AiSettings, UpdateAiSettingsRequest},
    },
    database,
    decision::{self, CreateDecisionRequest},
    locale::Locale,
    market_data::{MarketDataError, MarketDataProvider, MarketQuote},
    memo::{self, CreateMemoRequest},
    portfolio::{self, PortfolioImportCommitRequest, PortfolioImportPreviewRequest},
    profile,
};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

async fn test_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("connect sqlite");
    database::migrate(&pool).await.expect("migrate");
    pool
}

fn sample_import_content() -> String {
    [
        "symbol,name,quantity,average cost,currency,sector,market value",
        "AAPL,Apple,2,100,USD,Technology,250",
        "MSFT,Microsoft,1,200,USD,Technology,220",
    ]
    .join("\n")
}

#[tokio::test]
async fn portfolio_import_commit_computes_summary() {
    let pool = test_pool().await;
    let preview = portfolio::preview(PortfolioImportPreviewRequest {
        file_name: "positions.csv".to_string(),
        content: sample_import_content(),
        content_encoding: None,
    })
    .expect("preview");

    assert_eq!(preview.validation_errors, Vec::<String>::new());
    assert_eq!(preview.suggested_mapping.symbol, "symbol");

    let result = portfolio::commit_import(
        &pool,
        PortfolioImportCommitRequest {
            file_name: "positions.csv".to_string(),
            content: sample_import_content(),
            content_encoding: None,
            mapping: preview.suggested_mapping,
        },
    )
    .await
    .expect("commit");

    assert_eq!(result.imported_count, 2);

    let summary = portfolio::summary(&pool).await.expect("summary");
    assert_eq!(summary.positions_count, 2);
    assert_eq!(summary.total_market_value, 470.0);
    assert_eq!(summary.total_unrealized_pnl, 70.0);
}

#[tokio::test]
async fn failed_price_refresh_marks_stale_and_keeps_value() {
    let pool = test_pool().await;
    let preview = portfolio::preview(PortfolioImportPreviewRequest {
        file_name: "positions.csv".to_string(),
        content: sample_import_content(),
        content_encoding: None,
    })
    .expect("preview");

    portfolio::commit_import(
        &pool,
        PortfolioImportCommitRequest {
            file_name: "positions.csv".to_string(),
            content: sample_import_content(),
            content_encoding: None,
            mapping: preview.suggested_mapping,
        },
    )
    .await
    .expect("commit");

    let before = portfolio::summary(&pool).await.expect("summary before");
    let result = portfolio::refresh_prices(&pool, Arc::new(FailingProvider))
        .await
        .expect("refresh");
    let after = portfolio::summary(&pool).await.expect("summary after");

    assert_eq!(result.failed, 2);
    assert_eq!(before.total_market_value, after.total_market_value);
    assert_eq!(after.price_stale_count, 2);
}

#[tokio::test]
async fn profile_accumulates_from_memos_decisions_and_positions() {
    let pool = test_pool().await;
    let memo = memo::create(
        &pool,
        CreateMemoRequest {
            title: "Apple quality compounder".to_string(),
            symbol: Some("AAPL".to_string()),
            asset_type: None,
            thesis: Some("Durable ecosystem and recurring services economics.".to_string()),
            risks: Some("Valuation and regulatory pressure.".to_string()),
            catalysts: None,
            disconfirming_evidence: Some("Hardware cycle breaks unexpectedly.".to_string()),
            notes: None,
            status: None,
            tags: Some(vec!["quality".to_string()]),
        },
    )
    .await
    .expect("memo");

    decision::create(
        &pool,
        CreateDecisionRequest {
            memo_id: Some(memo.id),
            symbol: Some("AAPL".to_string()),
            action: "watch".to_string(),
            rationale: "Wait for a better risk/reward entry.".to_string(),
            confidence: 0.65,
            expected_outcome: "Track margin and services mix.".to_string(),
            review_date: Some("2026-09-30".to_string()),
        },
    )
    .await
    .expect("decision");

    let preview = portfolio::preview(PortfolioImportPreviewRequest {
        file_name: "positions.csv".to_string(),
        content: sample_import_content(),
        content_encoding: None,
    })
    .expect("preview");
    portfolio::commit_import(
        &pool,
        PortfolioImportCommitRequest {
            file_name: "positions.csv".to_string(),
            content: sample_import_content(),
            content_encoding: None,
            mapping: preview.suggested_mapping,
        },
    )
    .await
    .expect("commit");

    let profile = profile::calculate(&pool).await.expect("profile");

    assert!(profile.xp >= 100);
    assert!(profile
        .badges
        .iter()
        .any(|badge| badge.name == "First Memo"));
    assert!(profile
        .attributes
        .iter()
        .any(|attribute| attribute.name == "Decision Discipline" && attribute.score > 0));

    let zh_profile = profile::calculate_with_locale(&pool, Locale::Zh)
        .await
        .expect("zh profile");
    assert!(zh_profile
        .attributes
        .iter()
        .any(|attribute| attribute.name == "决策纪律" && attribute.score > 0));
}

#[test]
fn ai_settings_update_can_persist_env_without_echoing_secret() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let env_path = temp_dir.path().join(".env");
    let runtime = AiRuntime::new(
        AiSettings {
            provider: AiProviderKind::Mock,
            openai_api_key: None,
            openai_base_url: "https://api.openai.com/v1".to_string(),
            openai_model: "gpt-4.1-mini".to_string(),
            cli: CliSettings {
                provider: CliProviderKind::Codex,
                path: "codex".to_string(),
                model: None,
                profile: None,
            },
        },
        &env_path,
    );

    let response = runtime
        .update(UpdateAiSettingsRequest {
            provider: Some("cli".to_string()),
            openai_api_key: Some("sk-test".to_string()),
            openai_base_url: None,
            openai_model: None,
            cli_provider: Some("codex".to_string()),
            cli_path: Some("codex".to_string()),
            cli_model: Some("gpt-5.4".to_string()),
            cli_profile: Some("personal".to_string()),
            persist_to_env: Some(true),
        })
        .expect("update settings");

    assert_eq!(response.provider, "cli");
    assert_eq!(response.cli_provider, "codex");
    assert!(response.has_openai_api_key);

    let env = std::fs::read_to_string(env_path).expect("env file");
    assert!(env.contains("AI_PROVIDER=cli"));
    assert!(env.contains("OPENAI_API_KEY=sk-test"));
    assert!(env.contains("AI_CLI_PROVIDER=codex"));
    assert!(env.contains("AI_CLI_MODEL=gpt-5.4"));
    assert!(env.contains("AI_CLI_PROFILE=personal"));
}

struct FailingProvider;

#[async_trait]
impl MarketDataProvider for FailingProvider {
    async fn quote(&self, symbol: &str) -> Result<MarketQuote, MarketDataError> {
        Err(MarketDataError::Provider(format!("{symbol} unavailable")))
    }
}
