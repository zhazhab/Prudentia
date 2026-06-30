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
    profile, research,
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

#[tokio::test]
async fn research_records_can_be_created_listed_filtered_and_loaded() {
    let pool = test_pool().await;

    let record = research::create_record(
        &pool,
        research::CreateResearchRecord {
            kind: research::ResearchRecordKind::Distillation,
            title: "Munger mental models".to_string(),
            source_type: Some("person".to_string()),
            source_title: Some("Poor Charlie notes".to_string()),
            source_author: Some("Charlie Munger".to_string()),
            source_content: Some("Invert, always invert.".to_string()),
            symbol: Some("BRK.B".to_string()),
            memo_id: None,
            analysis: research::ResearchAnalysis {
                summary: "A checklist-oriented operating system for judgment.".to_string(),
                insights: vec!["Invert problems before acting.".to_string()],
                risks: vec!["Mental models can become slogans.".to_string()],
                checklist: vec!["Name the inversion before sizing.".to_string()],
                candidate_principles: vec!["Invert before underwriting.".to_string()],
                candidate_checklist_items: vec!["What would make this thesis fail?".to_string()],
            },
        },
    )
    .await
    .expect("create research record");

    assert_eq!(record.kind, research::ResearchRecordKind::Distillation);
    assert_eq!(record.symbol.as_deref(), Some("BRK.B"));
    assert_eq!(record.insights, vec!["Invert problems before acting."]);

    let all_records = research::list_records(&pool, research::ResearchRecordQuery::default())
        .await
        .expect("list all research records");
    assert_eq!(all_records.len(), 1);

    let filtered = research::list_records(
        &pool,
        research::ResearchRecordQuery {
            kind: Some(research::ResearchRecordKind::Distillation),
            symbol: Some("brk.b".to_string()),
            q: Some("munger".to_string()),
        },
    )
    .await
    .expect("list filtered research records");
    assert_eq!(filtered.len(), 1);

    let loaded = research::get_record(&pool, &record.id)
        .await
        .expect("load research record");
    assert_eq!(loaded.summary, record.summary);
}

#[tokio::test]
async fn research_create_record_rejects_empty_required_fields() {
    let pool = test_pool().await;

    let empty_title = research::create_record(
        &pool,
        research_request(
            "   ",
            "A useful summary.",
            Some("AAPL"),
            research::ResearchRecordKind::Distillation,
        ),
    )
    .await
    .expect_err("empty title should fail");
    assert!(format!("{empty_title:?}").contains("title is required"));

    let empty_summary = research::create_record(
        &pool,
        research_request(
            "Valid title",
            "   ",
            Some("AAPL"),
            research::ResearchRecordKind::Distillation,
        ),
    )
    .await
    .expect_err("empty summary should fail");
    assert!(format!("{empty_summary:?}").contains("analysis summary is required"));
}

#[tokio::test]
async fn research_create_record_trims_optionals_normalizes_symbol_and_preserves_raw_output() {
    let pool = test_pool().await;
    let mut request = research_request(
        "  Trimmed title  ",
        "  Trimmed summary  ",
        Some(" brk.b "),
        research::ResearchRecordKind::Distillation,
    );
    request.source_type = Some(" person ".to_string());
    request.source_title = Some("   ".to_string());
    request.source_author = Some(" Charlie Munger ".to_string());
    request.source_content = Some(" \n ".to_string());

    let record = research::create_record(&pool, request)
        .await
        .expect("create record");

    assert_eq!(record.title, "Trimmed title");
    assert_eq!(record.summary, "Trimmed summary");
    assert_eq!(record.source_type.as_deref(), Some("person"));
    assert_eq!(record.source_title, None);
    assert_eq!(record.source_author.as_deref(), Some("Charlie Munger"));
    assert_eq!(record.source_content, None);
    assert_eq!(record.symbol.as_deref(), Some("BRK.B"));
    assert_eq!(
        record.raw_output["summary"],
        serde_json::Value::String("  Trimmed summary  ".to_string())
    );
    assert_eq!(
        record.raw_output["insights"][0],
        serde_json::Value::String("Durability matters.".to_string())
    );
}

#[tokio::test]
async fn research_get_record_missing_id_reports_not_found() {
    let pool = test_pool().await;

    let error = research::get_record(&pool, "missing-research-id")
        .await
        .expect_err("missing id should fail");

    assert!(format!("{error:?}").contains("research record not found"));
}

#[tokio::test]
async fn research_list_records_filters_negatively_and_searches_summary_source_title_and_author() {
    let pool = test_pool().await;
    let summary_match = research::create_record(
        &pool,
        research_request(
            "Quality memo",
            "Durable moat with pricing power.",
            Some("AAPL"),
            research::ResearchRecordKind::StockSnapshot,
        ),
    )
    .await
    .expect("summary match");

    let mut source_title_request = research_request(
        "Letter notes",
        "Capital allocation notes.",
        Some("BRK.B"),
        research::ResearchRecordKind::Distillation,
    );
    source_title_request.source_title = Some("Annual Letter archive".to_string());
    let source_title_match = research::create_record(&pool, source_title_request)
        .await
        .expect("source title match");

    let mut author_request = research_request(
        "Cycle notes",
        "Risk control notes.",
        Some("OAK"),
        research::ResearchRecordKind::Distillation,
    );
    author_request.source_author = Some("Howard Marks".to_string());
    let author_match = research::create_record(&pool, author_request)
        .await
        .expect("author match");

    let no_q_matches = research::list_records(
        &pool,
        research::ResearchRecordQuery {
            q: Some("not present".to_string()),
            ..research::ResearchRecordQuery::default()
        },
    )
    .await
    .expect("negative q filter");
    assert!(no_q_matches.is_empty());

    let wrong_kind = research::list_records(
        &pool,
        research::ResearchRecordQuery {
            kind: Some(research::ResearchRecordKind::PortfolioReview),
            ..research::ResearchRecordQuery::default()
        },
    )
    .await
    .expect("negative kind filter");
    assert!(wrong_kind.is_empty());

    let wrong_symbol = research::list_records(
        &pool,
        research::ResearchRecordQuery {
            symbol: Some("MSFT".to_string()),
            ..research::ResearchRecordQuery::default()
        },
    )
    .await
    .expect("negative symbol filter");
    assert!(wrong_symbol.is_empty());

    let summary_matches = research::list_records(
        &pool,
        research::ResearchRecordQuery {
            q: Some("moat".to_string()),
            ..research::ResearchRecordQuery::default()
        },
    )
    .await
    .expect("summary q filter");
    assert_eq!(summary_matches[0].id, summary_match.id);

    let source_title_matches = research::list_records(
        &pool,
        research::ResearchRecordQuery {
            q: Some("annual letter".to_string()),
            ..research::ResearchRecordQuery::default()
        },
    )
    .await
    .expect("source title q filter");
    assert_eq!(source_title_matches[0].id, source_title_match.id);

    let author_matches = research::list_records(
        &pool,
        research::ResearchRecordQuery {
            q: Some("howard".to_string()),
            ..research::ResearchRecordQuery::default()
        },
    )
    .await
    .expect("source author q filter");
    assert_eq!(author_matches[0].id, author_match.id);
}

#[tokio::test]
async fn research_list_records_orders_by_updated_at_desc() {
    let pool = test_pool().await;
    let older = research::create_record(
        &pool,
        research_request(
            "Older record",
            "Older summary.",
            Some("AAPL"),
            research::ResearchRecordKind::Distillation,
        ),
    )
    .await
    .expect("older record");
    let newer = research::create_record(
        &pool,
        research_request(
            "Newer record",
            "Newer summary.",
            Some("MSFT"),
            research::ResearchRecordKind::Distillation,
        ),
    )
    .await
    .expect("newer record");

    sqlx::query("UPDATE research_records SET updated_at = ? WHERE id = ?")
        .bind("2026-01-01T00:00:00Z")
        .bind(&older.id)
        .execute(&pool)
        .await
        .expect("age older record");
    sqlx::query("UPDATE research_records SET updated_at = ? WHERE id = ?")
        .bind("2026-02-01T00:00:00Z")
        .bind(&newer.id)
        .execute(&pool)
        .await
        .expect("age newer record");

    let records = research::list_records(&pool, research::ResearchRecordQuery::default())
        .await
        .expect("ordered records");

    assert_eq!(records.len(), 2);
    assert_eq!(records[0].id, newer.id);
    assert_eq!(records[1].id, older.id);
}

#[tokio::test]
async fn research_corrupt_array_json_falls_back_to_empty_arrays_and_raw_output_json_is_strict() {
    let pool = test_pool().await;
    let record = research::create_record(
        &pool,
        research_request(
            "Corruption test",
            "Summary survives.",
            Some("AAPL"),
            research::ResearchRecordKind::Distillation,
        ),
    )
    .await
    .expect("create record");

    sqlx::query("UPDATE research_records SET insights_json = ? WHERE id = ?")
        .bind("not-json")
        .bind(&record.id)
        .execute(&pool)
        .await
        .expect("corrupt insights json");

    let loaded = research::get_record(&pool, &record.id)
        .await
        .expect("array corruption should be tolerated");
    assert!(loaded.insights.is_empty());

    sqlx::query("UPDATE research_records SET raw_output_json = ? WHERE id = ?")
        .bind("not-json")
        .bind(&record.id)
        .execute(&pool)
        .await
        .expect("corrupt raw output json");

    research::get_record(&pool, &record.id)
        .await
        .expect_err("raw output corruption should fail");
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

fn research_request(
    title: &str,
    summary: &str,
    symbol: Option<&str>,
    kind: research::ResearchRecordKind,
) -> research::CreateResearchRecord {
    research::CreateResearchRecord {
        kind,
        title: title.to_string(),
        source_type: None,
        source_title: None,
        source_author: None,
        source_content: None,
        symbol: symbol.map(str::to_string),
        memo_id: None,
        analysis: research::ResearchAnalysis {
            summary: summary.to_string(),
            insights: vec!["Durability matters.".to_string()],
            risks: vec!["Valuation can compress.".to_string()],
            checklist: vec!["Check incentives.".to_string()],
            candidate_principles: vec!["Prefer durable compounding.".to_string()],
            candidate_checklist_items: vec!["What breaks the moat?".to_string()],
        },
    }
}

struct FailingProvider;

#[async_trait]
impl MarketDataProvider for FailingProvider {
    async fn quote(&self, symbol: &str) -> Result<MarketQuote, MarketDataError> {
        Err(MarketDataError::Provider(format!("{symbol} unavailable")))
    }
}
