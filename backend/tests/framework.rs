use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
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
    profile, research, startup,
};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use tower::ServiceExt;

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
async fn portfolio_image_preview_returns_drafts_without_persisting_positions() {
    let pool = test_pool().await;
    let app = startup::build_router(
        pool.clone(),
        Arc::new(mock_ai_runtime()),
        Arc::new(FailingProvider),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/portfolio/import/image/preview")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{
                      "file_name":"positions.png",
                      "content":"aW1hZ2U=",
                      "content_encoding":"base64",
                      "mime_type":"image/png"
                    }"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let preview: serde_json::Value = serde_json::from_slice(&body).expect("json");

    assert_eq!(preview["source"], "codex_cli");
    assert_eq!(preview["rows"][0]["symbol"], "AAPL");
    assert_eq!(preview["rows"][0]["confidence"], "high");

    let summary = portfolio::summary(&pool).await.expect("summary");
    assert_eq!(summary.positions_count, 0);
}

#[tokio::test]
async fn portfolio_image_preview_rejects_unsupported_image_type() {
    let pool = test_pool().await;
    let app = startup::build_router(pool, Arc::new(mock_ai_runtime()), Arc::new(FailingProvider));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/portfolio/import/image/preview")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{
                      "file_name":"positions.gif",
                      "content":"aW1hZ2U=",
                      "content_encoding":"base64",
                      "mime_type":"image/gif"
                    }"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn portfolio_image_preview_rejects_non_base64_content() {
    let pool = test_pool().await;
    let app = startup::build_router(pool, Arc::new(mock_ai_runtime()), Arc::new(FailingProvider));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/portfolio/import/image/preview")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{
                      "file_name":"positions.png",
                      "content":"not-base64",
                      "content_encoding":"base64",
                      "mime_type":"image/png"
                    }"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
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
async fn research_distillation_workflow_saves_record() {
    let pool = test_pool().await;
    let ai = Arc::new(mock_ai_runtime());

    let record = research::distill(
        &pool,
        ai,
        research::DistillResearchRequest {
            title: "Munger notes".to_string(),
            source_type: Some("person".to_string()),
            source_title: Some("Interview notes".to_string()),
            source_author: Some("Charlie Munger".to_string()),
            source_content: "Invert before deciding.".to_string(),
            symbol: Some(" brk.b ".to_string()),
        },
        Locale::En,
    )
    .await
    .expect("distill research source");

    assert_eq!(record.kind, research::ResearchRecordKind::Distillation);
    assert_eq!(record.symbol.as_deref(), Some("BRK.B"));
    assert!(!record.candidate_principles.is_empty());
}

#[tokio::test]
async fn research_routes_create_and_list_distillations() {
    let pool = test_pool().await;
    let ai = Arc::new(AiRuntime::new(
        AiSettings {
            provider: AiProviderKind::Mock,
            openai_api_key: None,
            openai_base_url: "https://api.openai.com/v1".to_string(),
            openai_model: "gpt-4.1-mini".to_string(),
            cli: CliSettings::default(),
        },
        ".env",
    ));
    let app = startup::build_router(pool, ai, Arc::new(FailingProvider));

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/research/distill")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{
                      "title":"Munger notes",
                      "source_type":"person",
                      "source_title":"Interview",
                      "source_author":"Charlie Munger",
                      "source_content":"Invert before deciding.",
                      "symbol":"BRK.B"
                    }"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(create_response.status(), StatusCode::OK);
    let body = to_bytes(create_response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let created: research::ResearchRecord = serde_json::from_slice(&body).expect("json");
    assert_eq!(created.kind, research::ResearchRecordKind::Distillation);
    assert_eq!(created.symbol.as_deref(), Some("BRK.B"));
    assert_eq!(created.title, "Munger notes");

    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/research/records?kind=distillation&symbol=BRK.B&q=munger")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(list_response.status(), StatusCode::OK);
    let body = to_bytes(list_response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let records: Vec<research::ResearchRecord> = serde_json::from_slice(&body).expect("json");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].kind, research::ResearchRecordKind::Distillation);
    assert_eq!(records[0].symbol.as_deref(), Some("BRK.B"));
    assert_eq!(records[0].title, "Munger notes");

    let empty_kind_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/research/records?kind=")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(empty_kind_response.status(), StatusCode::OK);

    let invalid_kind_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/research/records?kind=nope")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(invalid_kind_response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(invalid_kind_response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let error: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(error["error"], "invalid research record kind");
}

#[tokio::test]
async fn research_portfolio_review_requires_positions() {
    let pool = test_pool().await;
    let ai = Arc::new(mock_ai_runtime());

    let error = research::review_portfolio(&pool, ai, Locale::En)
        .await
        .expect_err("empty portfolio should fail");

    assert!(format!("{error:?}").contains("portfolio has no positions"));
}

#[tokio::test]
async fn research_stock_snapshot_rejects_selected_memo_for_different_symbol() {
    let pool = test_pool().await;
    let memo = memo::create(
        &pool,
        CreateMemoRequest {
            title: "Apple thesis".to_string(),
            symbol: Some("AAPL".to_string()),
            asset_type: None,
            thesis: Some("Services durability.".to_string()),
            risks: None,
            catalysts: None,
            disconfirming_evidence: None,
            notes: None,
            status: None,
            tags: None,
        },
    )
    .await
    .expect("memo");

    let error = research::analyze_stock_snapshot(
        &pool,
        Arc::new(mock_ai_runtime()),
        Arc::new(FailingProvider),
        research::StockSnapshotRequest {
            symbol: "MSFT".to_string(),
            memo_id: Some(memo.id),
        },
        Locale::En,
    )
    .await
    .expect_err("mismatched selected memo should fail");

    assert!(format!("{error:?}").contains("selected memo does not match symbol"));
}

#[tokio::test]
async fn research_stock_snapshot_saves_context_with_matching_memo_and_quote_error() {
    let pool = test_pool().await;
    let selected = memo::create(
        &pool,
        CreateMemoRequest {
            title: "Berkshire thesis".to_string(),
            symbol: Some("brk.b".to_string()),
            asset_type: None,
            thesis: Some("Decentralized capital allocation.".to_string()),
            risks: None,
            catalysts: None,
            disconfirming_evidence: None,
            notes: None,
            status: None,
            tags: None,
        },
    )
    .await
    .expect("selected memo");
    memo::create(
        &pool,
        CreateMemoRequest {
            title: "Apple thesis".to_string(),
            symbol: Some("AAPL".to_string()),
            asset_type: None,
            thesis: Some("Services durability.".to_string()),
            risks: None,
            catalysts: None,
            disconfirming_evidence: None,
            notes: None,
            status: None,
            tags: None,
        },
    )
    .await
    .expect("other memo");

    let record = research::analyze_stock_snapshot(
        &pool,
        Arc::new(mock_ai_runtime()),
        Arc::new(FailingProvider),
        research::StockSnapshotRequest {
            symbol: " brk.b ".to_string(),
            memo_id: Some(selected.id.clone()),
        },
        Locale::En,
    )
    .await
    .expect("stock snapshot");

    assert_eq!(record.kind, research::ResearchRecordKind::StockSnapshot);
    assert_eq!(record.symbol.as_deref(), Some("BRK.B"));
    assert_eq!(record.memo_id.as_deref(), Some(selected.id.as_str()));

    let context: serde_json::Value =
        serde_json::from_str(record.source_content.as_deref().expect("source content"))
            .expect("source content json");
    assert_eq!(context["selected_memo"]["id"], selected.id);
    assert_eq!(context["quote_error"], "BRK.B unavailable");

    let related_symbols = context["related_memos"]
        .as_array()
        .expect("related memos array")
        .iter()
        .map(|memo| memo["symbol"].as_str().unwrap_or_default().to_string())
        .collect::<Vec<_>>();
    assert_eq!(related_symbols, vec!["brk.b".to_string()]);
}

#[tokio::test]
async fn research_portfolio_review_source_content_tracks_holdings_without_memos() {
    let pool = test_pool().await;
    let preview = portfolio::preview(PortfolioImportPreviewRequest {
        file_name: "positions.csv".to_string(),
        content: [
            "symbol,name,quantity,average cost,currency,sector,market value",
            "AAPL,Apple,2,100,USD,Technology,250",
            "MSFT,Microsoft,1,200,USD,Technology,220",
            "TSLA,Tesla,1,150,USD,Consumer Discretionary,180",
        ]
        .join("\n"),
        content_encoding: None,
    })
    .expect("preview");
    portfolio::commit_import(
        &pool,
        PortfolioImportCommitRequest {
            file_name: "positions.csv".to_string(),
            content: [
                "symbol,name,quantity,average cost,currency,sector,market value",
                "AAPL,Apple,2,100,USD,Technology,250",
                "MSFT,Microsoft,1,200,USD,Technology,220",
                "TSLA,Tesla,1,150,USD,Consumer Discretionary,180",
            ]
            .join("\n"),
            content_encoding: None,
            mapping: preview.suggested_mapping,
        },
    )
    .await
    .expect("commit");
    memo::create(
        &pool,
        CreateMemoRequest {
            title: "Apple thesis".to_string(),
            symbol: Some("aapl".to_string()),
            asset_type: None,
            thesis: Some("Services durability.".to_string()),
            risks: None,
            catalysts: None,
            disconfirming_evidence: None,
            notes: None,
            status: None,
            tags: None,
        },
    )
    .await
    .expect("aapl memo");
    memo::create(
        &pool,
        CreateMemoRequest {
            title: "Microsoft blank thesis".to_string(),
            symbol: Some("MSFT".to_string()),
            asset_type: None,
            thesis: Some("   ".to_string()),
            risks: None,
            catalysts: None,
            disconfirming_evidence: None,
            notes: None,
            status: None,
            tags: None,
        },
    )
    .await
    .expect("msft memo");

    let record = research::review_portfolio(&pool, Arc::new(mock_ai_runtime()), Locale::En)
        .await
        .expect("portfolio review");

    let context: serde_json::Value =
        serde_json::from_str(record.source_content.as_deref().expect("source content"))
            .expect("source content json");
    let holdings = context["holdings_without_memo"]
        .as_array()
        .expect("holdings array")
        .iter()
        .map(|value| value.as_str().unwrap_or_default().to_string())
        .collect::<Vec<_>>();

    assert!(holdings.contains(&"MSFT".to_string()));
    assert!(holdings.contains(&"TSLA".to_string()));
    assert!(!holdings.contains(&"AAPL".to_string()));
}

#[tokio::test]
async fn research_adoption_merges_candidates_without_duplicates() {
    let pool = test_pool().await;
    let mut request = research_request(
        "Adoption source",
        "Candidate summary.",
        Some("BRK.B"),
        research::ResearchRecordKind::Distillation,
    );
    request.analysis.candidate_principles = vec!["Invert before underwriting.".to_string()];
    request.analysis.candidate_checklist_items =
        vec!["What would make this thesis fail?".to_string()];
    let record = research::create_record(&pool, request)
        .await
        .expect("create research record");

    let system = research::adopt_candidates(
        &pool,
        &record.id,
        research::AdoptResearchCandidatesRequest {
            principles: vec![
                "  Invert before underwriting.  ".to_string(),
                "Invert before underwriting.".to_string(),
                "Buy because it is down.".to_string(),
            ],
            checklist_items: vec![
                " What would make this thesis fail? ".to_string(),
                "What would make this thesis fail?".to_string(),
                "Ignore valuation.".to_string(),
            ],
        },
        Locale::En,
    )
    .await
    .expect("adopt matching candidates");

    assert!(system
        .principles
        .contains(&"Invert before underwriting.".to_string()));
    assert!(system
        .checklist_items
        .contains(&"What would make this thesis fail?".to_string()));
    assert_eq!(
        system
            .principles
            .iter()
            .filter(|item| item.as_str() == "Invert before underwriting.")
            .count(),
        1
    );
    assert_eq!(
        system
            .checklist_items
            .iter()
            .filter(|item| item.as_str() == "What would make this thesis fail?")
            .count(),
        1
    );

    let error = research::adopt_candidates(
        &pool,
        &record.id,
        research::AdoptResearchCandidatesRequest {
            principles: vec!["Buy because it is down.".to_string()],
            checklist_items: Vec::new(),
        },
        Locale::En,
    )
    .await
    .expect_err("unknown candidates should fail");

    assert!(format!("{error:?}").contains("no selected candidates"));
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

#[tokio::test]
async fn mock_ai_returns_structured_research_analysis() {
    let provider = prudentia_backend::ai::mock::MockAiProvider;
    let analysis = prudentia_backend::ai::AiProvider::distill_research_source(
        &provider,
        &prudentia_backend::ai::ResearchSourceInput {
            title: "Munger notes".to_string(),
            source_type: Some("person".to_string()),
            source_title: Some("Interview notes".to_string()),
            source_author: Some("Charlie Munger".to_string()),
            source_content: "Invert before deciding.".to_string(),
            symbol: None,
        },
        Locale::En,
    )
    .await
    .expect("mock distillation");

    assert!(analysis.summary.contains("Munger notes"));
    assert_research_analysis_arrays_non_empty(&analysis);
}

#[test]
fn ai_research_analysis_uses_canonical_research_shape() {
    let analysis: prudentia_backend::research::ResearchAnalysis =
        prudentia_backend::ai::ResearchAnalysis {
            summary: "Summary".to_string(),
            insights: vec!["Insight".to_string()],
            risks: vec!["Risk".to_string()],
            checklist: vec!["Checklist".to_string()],
            candidate_principles: vec!["Principle".to_string()],
            candidate_checklist_items: vec!["Checklist item".to_string()],
        };

    assert_eq!(analysis.summary, "Summary");
}

#[tokio::test]
async fn mock_ai_stock_snapshot_returns_structured_research_analysis() {
    let provider = prudentia_backend::ai::mock::MockAiProvider;
    let analysis = prudentia_backend::ai::AiProvider::analyze_stock_snapshot(
        &provider,
        &prudentia_backend::ai::StockSnapshotContext {
            symbol: "AAPL".to_string(),
            position: None,
            portfolio_summary: empty_portfolio_summary(),
            related_memos: Vec::new(),
            selected_memo: None,
            quote: None,
            quote_error: None,
        },
        Locale::En,
    )
    .await
    .expect("mock stock snapshot");

    assert!(analysis.summary.contains("AAPL"));
    assert_research_analysis_arrays_non_empty(&analysis);
}

#[tokio::test]
async fn mock_ai_portfolio_review_returns_structured_research_analysis() {
    let provider = prudentia_backend::ai::mock::MockAiProvider;
    let analysis = prudentia_backend::ai::AiProvider::review_portfolio_risk(
        &provider,
        &prudentia_backend::ai::PortfolioReviewContext {
            positions: vec![sample_position("AAPL"), sample_position("MSFT")],
            summary: empty_portfolio_summary(),
            holdings_without_memo: Vec::new(),
        },
        Locale::En,
    )
    .await
    .expect("mock portfolio review");

    assert!(analysis.summary.contains('2'));
    assert_research_analysis_arrays_non_empty(&analysis);
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

fn mock_ai_runtime() -> AiRuntime {
    AiRuntime::new(
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
        ".env.test",
    )
}

fn assert_research_analysis_arrays_non_empty(analysis: &prudentia_backend::ai::ResearchAnalysis) {
    assert!(!analysis.insights.is_empty());
    assert!(!analysis.risks.is_empty());
    assert!(!analysis.checklist.is_empty());
    assert!(!analysis.candidate_principles.is_empty());
    assert!(!analysis.candidate_checklist_items.is_empty());
}

fn empty_portfolio_summary() -> portfolio::PortfolioSummary {
    portfolio::PortfolioSummary {
        total_market_value: 0.0,
        total_cost: 0.0,
        total_unrealized_pnl: 0.0,
        positions_count: 0,
        price_stale_count: 0,
        top_positions: Vec::new(),
        sectors: Vec::new(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

fn sample_position(symbol: &str) -> portfolio::PortfolioPosition {
    portfolio::PortfolioPosition {
        symbol: symbol.to_string(),
        name: symbol.to_string(),
        asset_type: "stock".to_string(),
        quantity: 1.0,
        average_cost: 100.0,
        currency: "USD".to_string(),
        account: None,
        market: None,
        sector: None,
        notes: None,
        last_price: Some(100.0),
        market_value: 100.0,
        unrealized_pnl: 0.0,
        weight: 0.0,
        price_updated_at: None,
        price_stale: false,
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

struct FailingProvider;

#[async_trait]
impl MarketDataProvider for FailingProvider {
    async fn quote(&self, symbol: &str) -> Result<MarketQuote, MarketDataError> {
        Err(MarketDataError::Provider(format!("{symbol} unavailable")))
    }
}
