use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

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
    ai_ws::{AiWsClientMessage, AiWsServerMessage},
    conversation::{ConversationThreadDetail, StartRunResponse},
    database,
    decision::{self, CreateDecisionRequest},
    decision_delta::{self, DecisionDeltaReviewRequest, RefreshDecisionDeltasRequest},
    investment_system::{
        activate_rule_graph, evaluate_active_rule_graph, RuleEdge, RuleGraph, RuleGraphPatch,
        RuleNode,
    },
    locale::Locale,
    market_data::{
        mock::MockMarketDataProvider, ExchangeRate, MarketDataError, MarketDataProvider,
        MarketQuote,
    },
    memo::{self, CreateMemoRequest},
    memo_thread::{self, CreateMemoThreadMessageRequest},
    portfolio::{
        self, PortfolioDraftCommitRequest, PortfolioImageImportPreviewRequest,
        PortfolioImportCommitRequest, PortfolioImportDraftRequest, PortfolioImportPreviewRequest,
        UpdatePortfolioPositionRequest,
    },
    profile, research, startup,
};
use sqlx::{sqlite::SqlitePoolOptions, Row, SqlitePool};
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

#[tokio::test]
async fn memo_thread_routes_list_detail_archive_and_soft_delete_threads() {
    let pool = test_pool().await;
    let thread = memo_thread::create_thread_with_user_message(
        &pool,
        CreateMemoThreadMessageRequest {
            thread_id: None,
            client_thread_id: Some("client-thread-1".to_string()),
            content: "复盘腾讯广告复苏假设".to_string(),
            locale: Locale::Zh,
        },
    )
    .await
    .expect("create thread");

    let app = startup::build_router(
        pool.clone(),
        Arc::new(mock_ai_runtime()),
        Arc::new(MockMarketDataProvider),
    );

    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/memo-threads")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(list_response.status(), StatusCode::OK);
    let body = to_bytes(list_response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let listed: Vec<memo_thread::MemoThreadSummary> = serde_json::from_slice(&body).expect("json");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, thread.id);
    assert_eq!(listed[0].title, "复盘腾讯广告复苏假设");
    assert!(listed[0].archived_at.is_none());

    let detail_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/memo-threads/{}", thread.id))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(detail_response.status(), StatusCode::OK);
    let body = to_bytes(detail_response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let detail: memo_thread::MemoThreadDetail = serde_json::from_slice(&body).expect("json");
    assert_eq!(detail.thread.id, thread.id);
    assert_eq!(detail.messages.len(), 1);
    assert_eq!(
        detail.messages[0].role,
        memo_thread::MemoThreadMessageRole::User
    );
    assert_eq!(detail.messages[0].content, "复盘腾讯广告复苏假设");

    let archive_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/memo-threads/{}/archive", thread.id))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(archive_response.status(), StatusCode::OK);

    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/memo-threads")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(list_response.status(), StatusCode::OK);
    let body = to_bytes(list_response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let listed: Vec<memo_thread::MemoThreadSummary> = serde_json::from_slice(&body).expect("json");
    assert!(listed.is_empty());

    let delete_response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/memo-threads/{}", thread.id))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(delete_response.status(), StatusCode::OK);
    assert!(memo_thread::get_detail(&pool, &thread.id, 50, None)
        .await
        .is_err());
}

#[tokio::test]
async fn conversation_run_persists_events_and_natural_assistant_message() {
    let pool = test_pool().await;
    let app = startup::build_router(
        pool.clone(),
        Arc::new(mock_ai_runtime()),
        Arc::new(MockMarketDataProvider),
    );
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/conversation/runs")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{
                      "client_request_id":"conversation-test-1",
                      "client_thread_id":"client-conversation-test-1",
                      "content":"你好",
                      "locale":"zh-CN"
                    }"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let accepted: StartRunResponse = serde_json::from_slice(&body).expect("json");

    let mut detail = None;
    for _ in 0..30 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!(
                        "/api/conversation/threads/{}",
                        accepted.thread.thread.id
                    ))
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body");
        let current: ConversationThreadDetail = serde_json::from_slice(&body).expect("json");
        if current.thread.active_run.is_none() {
            detail = Some(current);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let detail = detail.expect("conversation should complete");
    assert_eq!(detail.messages.len(), 2);
    assert_eq!(detail.messages[0].content, "你好");
    assert!(!detail.messages[1].content.is_empty());
    assert_ne!(detail.messages[1].content, "...");
    assert_eq!(
        detail.latest_run.as_ref().map(|run| run.status.as_str()),
        Some("completed")
    );

    let event_types = sqlx::query_scalar::<_, String>(
        "SELECT event_type FROM conversation_run_events WHERE run_id = ? ORDER BY event_id",
    )
    .bind(&accepted.run.id)
    .fetch_all(&pool)
    .await
    .expect("events");
    assert!(event_types.contains(&"run.accepted".to_string()));
    assert!(event_types.contains(&"message.delta".to_string()));
    assert!(event_types.contains(&"run.completed".to_string()));

    let phases = sqlx::query_scalar::<_, String>(
        r#"SELECT json_extract(payload_json, '$.phase')
        FROM conversation_run_events
        WHERE run_id = ? AND event_type = 'run.phase'
        ORDER BY event_id"#,
    )
    .bind(&accepted.run.id)
    .fetch_all(&pool)
    .await
    .expect("run phases");
    assert!(
        !phases.contains(&"extracting_actions".to_string()),
        "casual greetings must not start a second action-projection model call"
    );
}

#[tokio::test]
async fn conversation_trade_uses_import_baseline_and_weighted_average_cost() {
    let pool = test_pool().await;
    let preview = portfolio::preview(PortfolioImportPreviewRequest {
        file_name: "positions.csv".to_string(),
        content: sample_import_content(),
        content_encoding: None,
    })
    .expect("preview");
    portfolio::commit_import(
        &pool,
        Arc::new(MockMarketDataProvider),
        PortfolioImportCommitRequest {
            file_name: "positions.csv".to_string(),
            content: sample_import_content(),
            content_encoding: None,
            mapping: preview.suggested_mapping,
        },
    )
    .await
    .expect("commit");

    let historical = portfolio::record_trade(
        &pool,
        Arc::new(MockMarketDataProvider),
        portfolio::TradeRecord {
            side: "buy".to_string(),
            symbol: "AAPL".to_string(),
            quantity: 10.0,
            price: 50.0,
            currency: "USD".to_string(),
            occurred_at: "2020-01-01".to_string(),
            fees: 0.0,
            account: None,
            notes: None,
            fx_rate: Some(7.0),
            fx_source: Some("test".to_string()),
            corrects_trade_id: None,
        },
        None,
    )
    .await
    .expect("historical trade");
    assert!(!historical.impacts_portfolio);

    let receipt = portfolio::record_trade(
        &pool,
        Arc::new(MockMarketDataProvider),
        portfolio::TradeRecord {
            side: "buy".to_string(),
            symbol: "AAPL".to_string(),
            quantity: 1.0,
            price: 110.0,
            currency: "USD".to_string(),
            occurred_at: "2030-01-01".to_string(),
            fees: 3.0,
            account: None,
            notes: None,
            fx_rate: Some(7.0),
            fx_source: Some("test".to_string()),
            corrects_trade_id: None,
        },
        None,
    )
    .await
    .expect("new trade");
    assert!(receipt.impacts_portfolio);
    let position = receipt.position.expect("position");
    assert_eq!(position.quantity, 3.0);
    assert!((position.average_cost - (313.0 / 3.0)).abs() < 0.000001);
    assert_eq!(receipt.amount_base, Some(791.0));
}

#[tokio::test]
async fn rule_graph_activates_a_validated_executable_version() {
    let pool = test_pool().await;
    let graph = RuleGraph {
        graph_id: "default".to_string(),
        name: "Valuation gate".to_string(),
        nodes: vec![
            RuleNode {
                id: "pe".to_string(),
                label: "PE".to_string(),
                kind: "fixed".to_string(),
                operation: "input".to_string(),
                config: serde_json::json!({ "key": "pe" }),
                input_schema: serde_json::Value::Null,
                output_schema: serde_json::json!({ "type": "number" }),
                x: 0.0,
                y: 0.0,
            },
            RuleNode {
                id: "gate".to_string(),
                label: "PE <= 20".to_string(),
                kind: "fixed".to_string(),
                operation: "compare".to_string(),
                config: serde_json::json!({ "operator": "lte", "value": 20.0 }),
                input_schema: serde_json::Value::Null,
                output_schema: serde_json::json!({ "type": "boolean" }),
                x: 120.0,
                y: 0.0,
            },
            RuleNode {
                id: "result".to_string(),
                label: "Result".to_string(),
                kind: "fixed".to_string(),
                operation: "output".to_string(),
                config: serde_json::Value::Null,
                input_schema: serde_json::Value::Null,
                output_schema: serde_json::json!({ "type": "boolean" }),
                x: 240.0,
                y: 0.0,
            },
        ],
        edges: vec![
            RuleEdge {
                id: "pe-gate".to_string(),
                from_node: "pe".to_string(),
                to_node: "gate".to_string(),
                condition: None,
            },
            RuleEdge {
                id: "gate-result".to_string(),
                from_node: "gate".to_string(),
                to_node: "result".to_string(),
                condition: None,
            },
        ],
    };
    let version = activate_rule_graph(
        &pool,
        RuleGraphPatch {
            base_version: 1,
            graph,
        },
        None,
    )
    .await
    .expect("activate graph");
    assert_eq!(version.version, 2);
    let evaluation = evaluate_active_rule_graph(&pool, serde_json::json!({ "pe": 18.0 }))
        .await
        .expect("evaluate graph");
    assert_eq!(evaluation.output, serde_json::Value::Bool(true));
}

#[tokio::test]
async fn decision_delta_migration_creates_timeline_indexes() {
    let pool = test_pool().await;

    assert!(table_names(&pool)
        .await
        .contains(&"portfolio_position_snapshots".to_string()));
    assert!(table_names(&pool)
        .await
        .contains(&"portfolio_cash_flows".to_string()));
    assert!(index_names(&pool, "portfolio_cash_flows")
        .await
        .contains(&"idx_portfolio_cash_flows_occurred_at".to_string()));
    assert!(index_names(&pool, "decision_delta_legs")
        .await
        .contains(&"idx_decision_delta_legs_decision_kind".to_string()));
    assert!(index_names(&pool, "decision_delta_snapshots")
        .await
        .contains(&"idx_decision_delta_snapshots_latest".to_string()));
    assert!(index_names(&pool, "decisions")
        .await
        .contains(&"idx_decisions_symbol".to_string()));
    assert!(index_names(&pool, "decisions")
        .await
        .contains(&"idx_decisions_action".to_string()));
    assert!(index_names(&pool, "decisions")
        .await
        .contains(&"idx_decisions_created_at".to_string()));
}

fn sample_import_content() -> String {
    [
        "symbol,name,quantity,average cost,currency,sector,market value",
        "AAPL,Apple,2,100,USD,Technology,250",
        "MSFT,Microsoft,1,200,USD,Technology,220",
    ]
    .join("\n")
}

#[test]
fn ai_ws_messages_round_trip_portfolio_image_import() {
    let parsed: AiWsClientMessage = serde_json::from_value(serde_json::json!({
        "type": "portfolio_image_import.start",
        "request_id": "req-1",
        "payload": {
            "file_name": "positions.png",
            "content": "aW1hZ2U=",
            "content_encoding": "base64",
            "mime_type": "image/png"
        }
    }))
    .expect("client message");

    match parsed {
        AiWsClientMessage::PortfolioImageImportStart {
            request_id,
            payload:
                PortfolioImageImportPreviewRequest {
                    file_name,
                    content_encoding,
                    ..
                },
        } => {
            assert_eq!(request_id, "req-1");
            assert_eq!(file_name, "positions.png");
            assert_eq!(content_encoding.as_deref(), Some("base64"));
        }
        other => panic!("unexpected message: {other:?}"),
    }

    let serialized = serde_json::to_value(AiWsServerMessage::Progress {
        request_id: "req-1".to_string(),
        stage: "recognizing_image".to_string(),
    })
    .expect("server message");

    assert_eq!(serialized["type"], "progress");
    assert_eq!(serialized["request_id"], "req-1");
    assert_eq!(serialized["stage"], "recognizing_image");
}

#[test]
fn ai_ws_messages_round_trip_memo_chat_events() {
    let parsed: AiWsClientMessage = serde_json::from_value(serde_json::json!({
        "type": "memo_chat.start",
        "request_id": "chat-1",
        "client_thread_id": "client-thread-1",
        "locale": "zh-CN",
        "message": {
            "content": "复盘腾讯"
        },
        "context_hints": {
            "last_thread_id": "last-thread"
        }
    }))
    .expect("client message");

    match parsed {
        AiWsClientMessage::MemoChatStart {
            request_id,
            thread_id,
            client_thread_id,
            locale,
            message,
            context_hints,
        } => {
            assert_eq!(request_id, "chat-1");
            assert!(thread_id.is_none());
            assert_eq!(client_thread_id.as_deref(), Some("client-thread-1"));
            assert_eq!(locale.as_deref(), Some("zh-CN"));
            assert_eq!(message.content, "复盘腾讯");
            assert_eq!(
                context_hints.and_then(|hints| hints.last_thread_id),
                Some("last-thread".to_string())
            );
        }
        other => panic!("unexpected message: {other:?}"),
    }

    let serialized = serde_json::to_value(AiWsServerMessage::Delta {
        request_id: "chat-1".to_string(),
        thread_id: "thread-1".to_string(),
        content: "第一段".to_string(),
    })
    .expect("server message");

    assert_eq!(serialized["type"], "delta");
    assert_eq!(serialized["request_id"], "chat-1");
    assert_eq!(serialized["thread_id"], "thread-1");
    assert_eq!(serialized["content"], "第一段");
}

async fn table_names(pool: &SqlitePool) -> Vec<String> {
    let rows = sqlx::query("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name ASC")
        .fetch_all(pool)
        .await
        .expect("table names");
    rows.into_iter()
        .map(|row| row.try_get::<String, _>("name").expect("table name"))
        .collect()
}

async fn index_names(pool: &SqlitePool, table: &str) -> Vec<String> {
    let escaped_table = table.replace('\'', "''");
    let rows = sqlx::query(&format!("PRAGMA index_list('{escaped_table}')"))
        .fetch_all(pool)
        .await
        .expect("index list");
    rows.into_iter()
        .map(|row| row.try_get::<String, _>("name").expect("index name"))
        .collect()
}

async fn seed_decision_delta_snapshots(pool: &SqlitePool, decision_id: &str, count: usize) {
    for index in 0..count {
        let created_at = format!("2026-01-01T00:00:00.{index:03}Z");
        sqlx::query(
            r#"
            INSERT INTO decision_delta_snapshots (
                id, decision_id, as_of_date, actual_value, baseline_value, delta_value,
                delta_pct, portfolio_impact_pct, price_used, price_source, price_updated_at,
                fx_rate_used, fx_source, fx_updated_at, price_stale, fx_stale, created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(format!("snapshot-{index:03}"))
        .bind(decision_id)
        .bind(&created_at)
        .bind(1000.0 + index as f64)
        .bind(1000.0)
        .bind(index as f64)
        .bind(Some(index as f64 / 1000.0))
        .bind(None::<f64>)
        .bind(Some(100.0 + index as f64))
        .bind(Some("seed-test"))
        .bind(Some(created_at.clone()))
        .bind(Some(1.0))
        .bind(Some("identity"))
        .bind(Some(created_at.clone()))
        .bind(false)
        .bind(false)
        .bind(&created_at)
        .execute(pool)
        .await
        .expect("insert snapshot");
    }
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
        Arc::new(MockMarketDataProvider),
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
        Arc::new(MockMarketDataProvider),
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
async fn mock_price_refresh_keeps_imported_values() {
    let pool = test_pool().await;
    let preview = portfolio::preview(PortfolioImportPreviewRequest {
        file_name: "positions.csv".to_string(),
        content: sample_import_content(),
        content_encoding: None,
    })
    .expect("preview");

    portfolio::commit_import(
        &pool,
        Arc::new(MockMarketDataProvider),
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
    let result = portfolio::refresh_prices(&pool, Arc::new(MockMarketDataProvider))
        .await
        .expect("refresh");
    let after = portfolio::summary(&pool).await.expect("summary after");

    assert_eq!(result.failed, 2);
    assert_eq!(before.total_market_value, after.total_market_value);
    assert_eq!(after.price_stale_count, 2);
}

#[tokio::test]
async fn price_refresh_matches_broker_position_pnl_formula() {
    let pool = test_pool().await;
    let preview = portfolio::preview(PortfolioImportPreviewRequest {
        file_name: "positions.csv".to_string(),
        content: sample_import_content(),
        content_encoding: None,
    })
    .expect("preview");

    portfolio::commit_import(
        &pool,
        Arc::new(StaticFxQuoteProvider {
            price: 120.0,
            currency: "USD",
            fx_rate: 7.0,
        }),
        PortfolioImportCommitRequest {
            file_name: "positions.csv".to_string(),
            content: sample_import_content(),
            content_encoding: None,
            mapping: preview.suggested_mapping,
        },
    )
    .await
    .expect("commit");

    portfolio::refresh_prices(
        &pool,
        Arc::new(StaticFxQuoteProvider {
            price: 120.0,
            currency: "USD",
            fx_rate: 7.0,
        }),
    )
    .await
    .expect("refresh prices");

    let positions = portfolio::list_positions(&pool).await.expect("positions");
    let apple = positions
        .iter()
        .find(|position| position.symbol == "AAPL")
        .expect("AAPL position");
    assert_eq!(apple.last_price, Some(120.0));
    assert_eq!(apple.market_value, 240.0);
    assert_eq!(apple.unrealized_pnl, 40.0);

    let microsoft = positions
        .iter()
        .find(|position| position.symbol == "MSFT")
        .expect("MSFT position");
    assert_eq!(microsoft.last_price, Some(120.0));
    assert_eq!(microsoft.market_value, 120.0);
    assert_eq!(microsoft.unrealized_pnl, -80.0);

    let summary = portfolio::summary_with_fx(
        &pool,
        Arc::new(StaticFxQuoteProvider {
            price: 120.0,
            currency: "USD",
            fx_rate: 7.0,
        }),
    )
    .await
    .expect("summary");
    assert_eq!(summary.total_market_value_base, (240.0 + 120.0) * 7.0);
    assert_eq!(summary.total_cost_base, (2.0 * 100.0 + 1.0 * 200.0) * 7.0);
    assert_eq!(summary.total_unrealized_pnl_base, (40.0 - 80.0) * 7.0);
}

#[tokio::test]
async fn positions_expose_current_and_period_return_rates() {
    let pool = test_pool().await;
    let preview = portfolio::preview(PortfolioImportPreviewRequest {
        file_name: "positions.csv".to_string(),
        content: sample_import_content(),
        content_encoding: None,
    })
    .expect("preview");

    portfolio::commit_import(
        &pool,
        Arc::new(StaticFxQuoteProvider {
            price: 100.0,
            currency: "USD",
            fx_rate: 7.0,
        }),
        PortfolioImportCommitRequest {
            file_name: "positions.csv".to_string(),
            content: sample_import_content(),
            content_encoding: None,
            mapping: preview.suggested_mapping,
        },
    )
    .await
    .expect("commit");

    portfolio::refresh_prices(
        &pool,
        Arc::new(StaticFxQuoteProvider {
            price: 120.0,
            currency: "USD",
            fx_rate: 7.0,
        }),
    )
    .await
    .expect("refresh prices");

    let positions = portfolio::list_positions_for_period(
        &pool,
        portfolio::PortfolioPositionsQuery {
            period: Some("since_inception".to_string()),
        },
    )
    .await
    .expect("positions");
    let apple = positions
        .iter()
        .find(|position| position.symbol == "AAPL")
        .expect("AAPL position");
    assert_eq!(apple.unrealized_pnl_pct, Some(0.2));
    assert_eq!(apple.period_profit_loss_base, Some(-70.0));
    assert_eq!(apple.period_return_pct, Some(-0.04));

    let microsoft = positions
        .iter()
        .find(|position| position.symbol == "MSFT")
        .expect("MSFT position");
    assert_eq!(microsoft.unrealized_pnl_pct, Some(-0.4));
    assert_eq!(microsoft.period_profit_loss_base, Some(-700.0));
    assert!(
        (microsoft.period_return_pct.expect("period return") - (-100.0 / 220.0)).abs() < 0.000001
    );
}

#[tokio::test]
async fn portfolio_performance_uses_snapshots_and_benchmark_proxies() {
    let pool = test_pool().await;
    let preview = portfolio::preview(PortfolioImportPreviewRequest {
        file_name: "positions.csv".to_string(),
        content: sample_import_content(),
        content_encoding: None,
    })
    .expect("preview");

    portfolio::commit_import(
        &pool,
        Arc::new(StaticFxQuoteProvider {
            price: 100.0,
            currency: "USD",
            fx_rate: 7.0,
        }),
        PortfolioImportCommitRequest {
            file_name: "positions.csv".to_string(),
            content: sample_import_content(),
            content_encoding: None,
            mapping: preview.suggested_mapping,
        },
    )
    .await
    .expect("commit");

    portfolio::record_portfolio_performance_snapshot(
        &pool,
        Arc::new(StaticFxQuoteProvider {
            price: 100.0,
            currency: "USD",
            fx_rate: 7.0,
        }),
        "price_refresh",
    )
    .await
    .expect("record price refresh snapshot");

    let performance = portfolio::portfolio_performance(
        &pool,
        portfolio::PortfolioPerformanceQuery {
            period: Some("since_inception".to_string()),
        },
    )
    .await
    .expect("performance");

    assert_eq!(performance.period, "since_inception");
    assert_eq!(performance.series.len(), 2);
    assert_eq!(performance.portfolio.profit_loss_base, Some(0.0));
    assert_eq!(performance.portfolio.annualized_return_pct, Some(0.0));
    assert_eq!(performance.series[0].annualized_return_pct, Some(0.0));
    assert_eq!(performance.benchmarks.len(), 3);
    assert!(performance
        .benchmarks
        .iter()
        .all(|benchmark| benchmark.available));
    assert!(performance
        .benchmarks
        .iter()
        .all(|benchmark| benchmark.annualized_return_pct == Some(0.0)));

    let serialized = serde_json::to_value(&performance).expect("serialized performance");
    let benchmark = serialized["benchmarks"]
        .as_array()
        .expect("benchmarks")
        .iter()
        .find(|benchmark| benchmark["key"] == "sp500")
        .expect("sp500 benchmark");
    assert_eq!(benchmark["source"], "static-fx-test");
    assert_eq!(benchmark["series"][0]["source"], "static-fx-test");
}

#[tokio::test]
async fn portfolio_performance_restarts_after_all_positions_are_deleted() {
    let pool = test_pool().await;
    let first_content = sample_import_content();
    let first_preview = portfolio::preview(PortfolioImportPreviewRequest {
        file_name: "positions.csv".to_string(),
        content: first_content.clone(),
        content_encoding: None,
    })
    .expect("first preview");
    portfolio::commit_import(
        &pool,
        Arc::new(MockMarketDataProvider),
        PortfolioImportCommitRequest {
            file_name: "positions.csv".to_string(),
            content: first_content,
            content_encoding: None,
            mapping: first_preview.suggested_mapping,
        },
    )
    .await
    .expect("first commit");

    portfolio::delete_position(&pool, Arc::new(MockMarketDataProvider), "AAPL")
        .await
        .expect("delete AAPL");
    portfolio::delete_position(&pool, Arc::new(MockMarketDataProvider), "MSFT")
        .await
        .expect("delete MSFT");

    let second_content = [
        "symbol,name,quantity,average cost,currency,sector,market value",
        "PDD,PDD Holdings,1,900,USD,Consumer,1000",
    ]
    .join("\n");
    let second_preview = portfolio::preview(PortfolioImportPreviewRequest {
        file_name: "positions.csv".to_string(),
        content: second_content.clone(),
        content_encoding: None,
    })
    .expect("second preview");
    portfolio::commit_import(
        &pool,
        Arc::new(MockMarketDataProvider),
        PortfolioImportCommitRequest {
            file_name: "positions.csv".to_string(),
            content: second_content,
            content_encoding: None,
            mapping: second_preview.suggested_mapping,
        },
    )
    .await
    .expect("second commit");

    let performance = portfolio::portfolio_performance(
        &pool,
        portfolio::PortfolioPerformanceQuery {
            period: Some("month".to_string()),
        },
    )
    .await
    .expect("performance");

    assert_eq!(performance.series.len(), 1);
    assert_eq!(performance.portfolio.profit_loss_base, Some(0.0));
    assert_eq!(performance.portfolio.return_pct, Some(0.0));
}

#[tokio::test]
async fn portfolio_performance_benchmark_failure_does_not_block_snapshot() {
    let pool = test_pool().await;
    let content = [
        "symbol,name,quantity,average cost,currency,sector,market value",
        "600519.SS,Kweichow Moutai,1,1500,CNY,Consumer,1600",
    ]
    .join("\n");
    let preview = portfolio::preview(PortfolioImportPreviewRequest {
        file_name: "positions.csv".to_string(),
        content: content.clone(),
        content_encoding: None,
    })
    .expect("preview");

    portfolio::commit_import(
        &pool,
        Arc::new(FailingProvider),
        PortfolioImportCommitRequest {
            file_name: "positions.csv".to_string(),
            content,
            content_encoding: None,
            mapping: preview.suggested_mapping,
        },
    )
    .await
    .expect("commit");

    let performance = portfolio::portfolio_performance(
        &pool,
        portfolio::PortfolioPerformanceQuery {
            period: Some("since_inception".to_string()),
        },
    )
    .await
    .expect("performance");

    assert_eq!(performance.series.len(), 1);
    assert!(performance
        .benchmarks
        .iter()
        .all(|benchmark| !benchmark.available && benchmark.stale));
}

#[tokio::test]
async fn portfolio_price_refresh_daily_ttl_skips_when_fresh() {
    let pool = test_pool().await;
    let provider = Arc::new(CountingFxProvider::new());
    let preview = portfolio::preview(PortfolioImportPreviewRequest {
        file_name: "positions.csv".to_string(),
        content: sample_import_content(),
        content_encoding: None,
    })
    .expect("preview");

    portfolio::commit_import(
        &pool,
        provider.clone(),
        PortfolioImportCommitRequest {
            file_name: "positions.csv".to_string(),
            content: sample_import_content(),
            content_encoding: None,
            mapping: preview.suggested_mapping,
        },
    )
    .await
    .expect("commit");

    let first = portfolio::refresh_prices_if_due(
        &pool,
        provider.clone(),
        std::time::Duration::from_secs(24 * 60 * 60),
    )
    .await
    .expect("first refresh");
    assert!(first.is_some());
    let quote_calls_after_first = provider.quote_calls.load(Ordering::SeqCst);

    let second = portfolio::refresh_prices_if_due(
        &pool,
        provider.clone(),
        std::time::Duration::from_secs(24 * 60 * 60),
    )
    .await
    .expect("second refresh");

    assert!(second.is_none());
    assert_eq!(
        provider.quote_calls.load(Ordering::SeqCst),
        quote_calls_after_first
    );
}

#[tokio::test]
async fn mock_price_refresh_repairs_positions_with_embedded_current_price() {
    let pool = test_pool().await;
    let preview = portfolio::draft_from_import(PortfolioImportDraftRequest {
        file_name: "positions.csv".to_string(),
        content: [
            "symbol,name,quantity,average cost,currency,market,market value,notes",
            "0700.HK,Tencent,900,489.877,HKD,HK,335646.34,current_price=430.200",
        ]
        .join("\n"),
        content_encoding: None,
        mapping: portfolio::PortfolioImportMapping {
            symbol: "symbol".to_string(),
            name: "name".to_string(),
            quantity: "quantity".to_string(),
            average_cost: "average cost".to_string(),
            currency: "currency".to_string(),
            market: Some("market".to_string()),
            imported_market_value: Some("market value".to_string()),
            notes: Some("notes".to_string()),
            ..Default::default()
        },
    })
    .expect("draft");

    portfolio::commit_draft_rows(
        &pool,
        Arc::new(MockMarketDataProvider),
        PortfolioDraftCommitRequest {
            rows: preview.draft_rows,
        },
    )
    .await
    .expect("commit");
    sqlx::query(
        "UPDATE portfolio_positions SET last_price = 62.1, market_value = 74520, price_stale = 0 WHERE symbol = '0700.HK'",
    )
    .execute(&pool)
    .await
    .expect("corrupt position");

    portfolio::refresh_prices(&pool, Arc::new(MockMarketDataProvider))
        .await
        .expect("refresh");

    let positions = portfolio::list_positions(&pool).await.expect("positions");
    assert_eq!(positions[0].last_price, Some(430.2));
    assert_eq!(positions[0].market_value, 387180.0);
    assert!(positions[0].price_stale);
}

#[tokio::test]
async fn decisions_can_be_listed_and_loaded() {
    let pool = test_pool().await;
    let created = decision::create(
        &pool,
        CreateDecisionRequest {
            memo_id: None,
            symbol: Some("aapl".to_string()),
            action: "buy".to_string(),
            rationale: "Services thesis.".to_string(),
            confidence: 0.7,
            expected_outcome: "Margin expansion.".to_string(),
            review_date: Some("2026-09-30".to_string()),
            decision_date: None,
            quantity: None,
            notional: None,
            price: None,
            currency: None,
            baseline_type: None,
            hypothetical_notional: None,
        },
    )
    .await
    .expect("create decision");

    let loaded = decision::get(&pool, &created.id)
        .await
        .expect("load decision");
    assert_eq!(loaded.symbol.as_deref(), Some("AAPL"));

    let decisions = decision::list(&pool).await.expect("list decisions");
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0].id, created.id);
}

#[tokio::test]
async fn decision_delta_buy_refresh_compares_asset_against_cash_baseline() {
    let pool = test_pool().await;
    let decision = quantified_decision(&pool, "buy", "AAPL", Some(10.0), None, 100.0).await;

    let detail = decision_delta::get_detail(&pool, &decision.id)
        .await
        .expect("delta detail");
    assert_eq!(detail.legs.len(), 2);
    assert!(detail
        .legs
        .iter()
        .any(|leg| leg.leg_kind == "actual" && leg.symbol.as_deref() == Some("AAPL")));
    assert!(detail
        .legs
        .iter()
        .any(|leg| leg.leg_kind == "baseline" && leg.baseline_type.as_deref() == Some("cash")));

    let result = decision_delta::refresh(
        &pool,
        Arc::new(StaticCnyProvider {
            price: 120.0,
            fail: false,
        }),
        RefreshDecisionDeltasRequest {
            decision_ids: Some(vec![decision.id.clone()]),
        },
    )
    .await
    .expect("refresh decision deltas");

    assert_eq!(result.refreshed, 1);
    let detail = decision_delta::get_detail(&pool, &decision.id)
        .await
        .expect("delta detail after refresh");
    let latest = detail.latest_snapshot.expect("latest snapshot");
    assert_eq!(latest.actual_value, 1200.0);
    assert_eq!(latest.baseline_value, 1000.0);
    assert_eq!(latest.delta_value, 200.0);
    assert_eq!(latest.delta_pct, Some(0.2));
    assert_eq!(detail.snapshots.len(), 1);
}

#[tokio::test]
async fn decision_delta_sell_refresh_compares_cash_against_continued_holding() {
    let pool = test_pool().await;
    let decision = quantified_decision(&pool, "sell", "AAPL", Some(10.0), None, 100.0).await;

    decision_delta::refresh(
        &pool,
        Arc::new(StaticCnyProvider {
            price: 120.0,
            fail: false,
        }),
        RefreshDecisionDeltasRequest {
            decision_ids: Some(vec![decision.id.clone()]),
        },
    )
    .await
    .expect("refresh decision deltas");

    let latest = decision_delta::get_detail(&pool, &decision.id)
        .await
        .expect("delta detail")
        .latest_snapshot
        .expect("latest snapshot");
    assert_eq!(latest.actual_value, 1000.0);
    assert_eq!(latest.baseline_value, 1200.0);
    assert_eq!(latest.delta_value, -200.0);
}

#[tokio::test]
async fn decision_delta_non_base_currency_uses_fx() {
    let pool = test_pool().await;
    let decision = decision::create(
        &pool,
        CreateDecisionRequest {
            memo_id: None,
            symbol: Some("AAPL".to_string()),
            action: "buy".to_string(),
            rationale: "Buy USD asset and compare against USD cash baseline.".to_string(),
            confidence: 0.7,
            expected_outcome: "Track CNY decision delta.".to_string(),
            review_date: Some("2026-09-30".to_string()),
            decision_date: Some("2026-01-01".to_string()),
            quantity: Some(10.0),
            notional: Some(1000.0),
            price: Some(100.0),
            currency: Some("USD".to_string()),
            baseline_type: None,
            hypothetical_notional: None,
        },
    )
    .await
    .expect("create usd decision");

    decision_delta::refresh(
        &pool,
        Arc::new(StaticFxQuoteProvider {
            price: 120.0,
            currency: "USD",
            fx_rate: 7.0,
        }),
        RefreshDecisionDeltasRequest {
            decision_ids: Some(vec![decision.id.clone()]),
        },
    )
    .await
    .expect("refresh decision deltas");

    let latest = decision_delta::get_detail(&pool, &decision.id)
        .await
        .expect("delta detail")
        .latest_snapshot
        .expect("latest snapshot");
    assert_eq!(latest.actual_value, 8400.0);
    assert_eq!(latest.baseline_value, 7000.0);
    assert_eq!(latest.delta_value, 1400.0);
    assert_eq!(latest.fx_rate_used, Some(7.0));
}

#[tokio::test]
async fn decision_delta_detail_limits_snapshot_history_by_default_and_query() {
    let pool = test_pool().await;
    let decision = quantified_decision(&pool, "buy", "AAPL", Some(10.0), None, 100.0).await;
    seed_decision_delta_snapshots(&pool, &decision.id, 120).await;

    let app = startup::build_router(
        pool.clone(),
        Arc::new(mock_ai_runtime()),
        Arc::new(StaticCnyProvider {
            price: 100.0,
            fail: false,
        }),
    );

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/decision-deltas/{}", decision.id))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let detail: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(detail["snapshots"].as_array().expect("snapshots").len(), 90);
    assert_eq!(detail["latest_snapshot"]["delta_value"], 119.0);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/api/decision-deltas/{}?snapshot_limit=30",
                    decision.id
                ))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let detail: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(detail["snapshots"].as_array().expect("snapshots").len(), 30);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/api/decision-deltas/{}?snapshot_limit=999",
                    decision.id
                ))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let detail: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(
        detail["snapshots"].as_array().expect("snapshots").len(),
        120
    );
}

#[tokio::test]
async fn decision_delta_refresh_reuses_quotes_and_fx_within_batch() {
    let pool = test_pool().await;
    let first = decision::create(
        &pool,
        CreateDecisionRequest {
            memo_id: None,
            symbol: Some("AAPL".to_string()),
            action: "buy".to_string(),
            rationale: "First USD decision.".to_string(),
            confidence: 0.7,
            expected_outcome: "Track delta.".to_string(),
            review_date: Some("2026-09-30".to_string()),
            decision_date: Some("2026-01-01".to_string()),
            quantity: Some(10.0),
            notional: Some(1000.0),
            price: Some(100.0),
            currency: Some("USD".to_string()),
            baseline_type: None,
            hypothetical_notional: None,
        },
    )
    .await
    .expect("first decision");
    let second = decision::create(
        &pool,
        CreateDecisionRequest {
            memo_id: None,
            symbol: Some("AAPL".to_string()),
            action: "buy".to_string(),
            rationale: "Second USD decision.".to_string(),
            confidence: 0.7,
            expected_outcome: "Track delta.".to_string(),
            review_date: Some("2026-09-30".to_string()),
            decision_date: Some("2026-01-01".to_string()),
            quantity: Some(5.0),
            notional: Some(500.0),
            price: Some(100.0),
            currency: Some("USD".to_string()),
            baseline_type: None,
            hypothetical_notional: None,
        },
    )
    .await
    .expect("second decision");
    let provider = Arc::new(CountingFxProvider::new());

    let result = decision_delta::refresh(
        &pool,
        provider.clone(),
        RefreshDecisionDeltasRequest {
            decision_ids: Some(vec![first.id, second.id]),
        },
    )
    .await
    .expect("refresh");

    assert_eq!(result.refreshed, 2);
    assert_eq!(provider.quote_calls.load(Ordering::SeqCst), 1);
    assert_eq!(provider.fx_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn decision_delta_skip_requires_hypothetical_notional_to_quantify() {
    let pool = test_pool().await;
    let skipped = quantified_decision(&pool, "skip", "NVDA", None, None, 100.0).await;
    let detail = decision_delta::get_detail(&pool, &skipped.id)
        .await
        .expect("skip detail");
    assert!(detail.legs.is_empty());
    assert!(!detail.quantifiable);

    let hypothetical = quantified_decision(&pool, "skip", "NVDA", None, Some(1000.0), 100.0).await;
    let detail = decision_delta::get_detail(&pool, &hypothetical.id)
        .await
        .expect("hypothetical detail");
    assert!(detail.quantifiable);
    assert!(detail.legs.iter().any(|leg| leg.leg_kind == "baseline"
        && leg.baseline_type.as_deref() == Some("hypothetical_buy")));
}

#[tokio::test]
async fn decision_delta_timeline_sums_visible_latest_snapshots() {
    let pool = test_pool().await;
    let buy = quantified_decision(&pool, "buy", "AAPL", Some(10.0), None, 100.0).await;
    let sell = quantified_decision(&pool, "sell", "MSFT", Some(10.0), None, 100.0).await;

    decision_delta::refresh(
        &pool,
        Arc::new(StaticCnyProvider {
            price: 120.0,
            fail: false,
        }),
        RefreshDecisionDeltasRequest { decision_ids: None },
    )
    .await
    .expect("refresh all");

    let timeline =
        decision_delta::timeline(&pool, decision_delta::DecisionDeltaTimelineQuery::default())
            .await
            .expect("timeline");
    assert_eq!(timeline.items.len(), 2);
    assert_eq!(timeline.summary.label, "sum_of_decision_deltas");
    assert_eq!(timeline.summary.sum_delta_value, 0.0);
    assert_eq!(timeline.summary.positive_delta_count, 1);
    assert_eq!(timeline.summary.negative_delta_count, 1);

    let filtered = decision_delta::timeline(
        &pool,
        decision_delta::DecisionDeltaTimelineQuery {
            symbol: Some("AAPL".to_string()),
            ..decision_delta::DecisionDeltaTimelineQuery::default()
        },
    )
    .await
    .expect("filtered timeline");
    assert_eq!(filtered.items.len(), 1);
    assert_eq!(filtered.items[0].decision.id, buy.id);
    assert_ne!(filtered.items[0].decision.id, sell.id);
    assert_eq!(filtered.summary.sum_delta_value, 200.0);
}

#[tokio::test]
async fn decision_delta_snapshots_preserve_history_and_provider_failure_marks_stale() {
    let pool = test_pool().await;
    let decision = quantified_decision(&pool, "buy", "AAPL", Some(10.0), None, 100.0).await;

    decision_delta::refresh(
        &pool,
        Arc::new(StaticCnyProvider {
            price: 120.0,
            fail: false,
        }),
        RefreshDecisionDeltasRequest {
            decision_ids: Some(vec![decision.id.clone()]),
        },
    )
    .await
    .expect("first refresh");
    decision_delta::refresh(
        &pool,
        Arc::new(StaticCnyProvider {
            price: 130.0,
            fail: false,
        }),
        RefreshDecisionDeltasRequest {
            decision_ids: Some(vec![decision.id.clone()]),
        },
    )
    .await
    .expect("second refresh");
    decision_delta::refresh(
        &pool,
        Arc::new(StaticCnyProvider {
            price: 0.0,
            fail: true,
        }),
        RefreshDecisionDeltasRequest {
            decision_ids: Some(vec![decision.id.clone()]),
        },
    )
    .await
    .expect("failed refresh marks stale");

    let detail = decision_delta::get_detail(&pool, &decision.id)
        .await
        .expect("detail");
    assert_eq!(detail.snapshots.len(), 3);
    let latest = detail.latest_snapshot.expect("latest");
    assert!(latest.price_stale);
    assert_eq!(latest.delta_value, 300.0);
}

#[tokio::test]
async fn decision_delta_review_adoption_and_profile_reward_process_not_returns() {
    let pool = test_pool().await;
    let decision = quantified_decision(&pool, "buy", "AAPL", Some(10.0), None, 100.0).await;
    decision_delta::refresh(
        &pool,
        Arc::new(StaticCnyProvider {
            price: 120.0,
            fail: false,
        }),
        RefreshDecisionDeltasRequest {
            decision_ids: Some(vec![decision.id.clone()]),
        },
    )
    .await
    .expect("refresh");

    let profile_after_positive_delta = profile::calculate(&pool).await.expect("profile");

    let review = decision_delta::save_review(
        &pool,
        &decision.id,
        DecisionDeltaReviewRequest {
            notes: "Good process, not just good outcome.".to_string(),
            thesis_evidence: vec!["Services margin expanded.".to_string()],
            disconfirming_evidence: vec!["Hardware cycle softened.".to_string()],
            lessons: vec!["Size slowly when baseline is cash.".to_string()],
            candidate_principles: vec!["Measure decision deltas before celebrating.".to_string()],
            candidate_checklist_items: vec!["What is the no-action baseline?".to_string()],
        },
    )
    .await
    .expect("save review");
    assert_eq!(review.candidate_principles.len(), 1);

    let system = decision_delta::adopt_candidates(
        &pool,
        &decision.id,
        decision_delta::AdoptDecisionDeltaCandidatesRequest {
            principles: vec!["Measure decision deltas before celebrating.".to_string()],
            checklist_items: vec!["What is the no-action baseline?".to_string()],
        },
        Locale::En,
    )
    .await
    .expect("adopt candidates");
    assert!(system
        .principles
        .contains(&"Measure decision deltas before celebrating.".to_string()));
    assert!(system
        .checklist_items
        .contains(&"What is the no-action baseline?".to_string()));

    let profile_after_review = profile::calculate(&pool).await.expect("profile");
    assert!(profile_after_review.xp > profile_after_positive_delta.xp);

    let unknown = decision_delta::adopt_candidates(
        &pool,
        &decision.id,
        decision_delta::AdoptDecisionDeltaCandidatesRequest {
            principles: vec!["Invented principle".to_string()],
            checklist_items: Vec::new(),
        },
        Locale::En,
    )
    .await
    .expect_err("unknown candidates fail");
    assert!(format!("{unknown:?}").contains("no selected candidates"));
}

#[tokio::test]
async fn portfolio_image_preview_returns_drafts_without_persisting_positions() {
    let preview = portfolio::preview_image_import(
        Arc::new(mock_ai_runtime()),
        PortfolioImageImportPreviewRequest {
            file_name: "positions.png".to_string(),
            content: "aW1hZ2U=".to_string(),
            content_encoding: Some("base64".to_string()),
            mime_type: Some("image/png".to_string()),
        },
    )
    .await
    .expect("preview");

    assert_eq!(preview.source, "codex_cli");
    assert_eq!(preview.rows[0].symbol, "AAPL");
    assert_eq!(preview.rows[0].confidence, "high");
}

#[tokio::test]
async fn portfolio_image_preview_json_route_is_not_exposed() {
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
                      "content":"aW1hZ2U=",
                      "content_encoding":"base64",
                      "mime_type":"image/png"
                    }"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn portfolio_image_preview_rejects_unsupported_image_type() {
    let error = portfolio::preview_image_import(
        Arc::new(mock_ai_runtime()),
        PortfolioImageImportPreviewRequest {
            file_name: "positions.gif".to_string(),
            content: "aW1hZ2U=".to_string(),
            content_encoding: Some("base64".to_string()),
            mime_type: Some("image/gif".to_string()),
        },
    )
    .await
    .expect_err("unsupported image type");

    assert!(format!("{error:?}").contains("unsupported image type"));
}

#[tokio::test]
async fn portfolio_image_preview_rejects_non_base64_content() {
    let error = portfolio::preview_image_import(
        Arc::new(mock_ai_runtime()),
        PortfolioImageImportPreviewRequest {
            file_name: "positions.png".to_string(),
            content: "not-base64".to_string(),
            content_encoding: Some("base64".to_string()),
            mime_type: Some("image/png".to_string()),
        },
    )
    .await
    .expect_err("bad base64 content");

    assert!(format!("{error:?}").contains("Invalid"));
}

#[tokio::test]
async fn portfolio_file_preview_returns_editable_draft_rows_with_inferred_markets() {
    let preview = portfolio::preview(PortfolioImportPreviewRequest {
        file_name: "positions.csv".to_string(),
        content: [
            "symbol,name,quantity,average cost,currency,market value",
            "AAPL,Apple,2,100,,250",
            "0700.HK,Tencent,100,300,,32000",
            "600519,Maotai,10,1600,,18000",
        ]
        .join("\n"),
        content_encoding: None,
    })
    .expect("preview");

    assert_eq!(preview.draft_rows.len(), 3);
    assert_eq!(preview.draft_rows[0].market, "US");
    assert_eq!(preview.draft_rows[0].currency, "USD");
    assert_eq!(preview.draft_rows[1].market, "HK");
    assert_eq!(preview.draft_rows[1].currency, "HKD");
    assert_eq!(preview.draft_rows[2].market, "CN");
    assert_eq!(preview.draft_rows[2].currency, "CNY");
    assert!(preview.draft_rows.iter().all(|row| row.errors.is_empty()));
}

#[tokio::test]
async fn portfolio_draft_commit_blocks_invalid_rows_but_allows_low_confidence_rows() {
    let pool = test_pool().await;
    let preview = portfolio::draft_from_import(PortfolioImportDraftRequest {
        file_name: "positions.csv".to_string(),
        content: [
            "symbol,name,quantity,average cost,currency,market value",
            "AAPL,Apple,-2,100,USD,250",
            "MSFT,Microsoft,1,200,USD,220",
        ]
        .join("\n"),
        content_encoding: None,
        mapping: portfolio::PortfolioImportMapping {
            symbol: "symbol".to_string(),
            name: "name".to_string(),
            quantity: "quantity".to_string(),
            average_cost: "average cost".to_string(),
            currency: "currency".to_string(),
            imported_market_value: Some("market value".to_string()),
            ..Default::default()
        },
    })
    .expect("draft");

    let error = portfolio::commit_draft_rows(
        &pool,
        Arc::new(MockMarketDataProvider),
        PortfolioDraftCommitRequest {
            rows: preview.draft_rows.clone(),
        },
    )
    .await
    .expect_err("invalid draft should fail");
    assert!(format!("{error:?}").contains("quantity must be greater than 0"));

    let mut low_confidence_row = preview.draft_rows[1].clone();
    low_confidence_row.confidence = "low".to_string();
    low_confidence_row.warnings = vec!["Low confidence OCR.".to_string()];
    let result = portfolio::commit_draft_rows(
        &pool,
        Arc::new(MockMarketDataProvider),
        PortfolioDraftCommitRequest {
            rows: vec![low_confidence_row],
        },
    )
    .await
    .expect("low confidence row can be committed");

    assert_eq!(result.imported_count, 1);
    assert_eq!(
        portfolio::summary(&pool)
            .await
            .expect("summary")
            .positions_count,
        1
    );
}

#[tokio::test]
async fn portfolio_draft_commit_merges_duplicate_symbols() {
    let pool = test_pool().await;
    let preview = portfolio::preview(PortfolioImportPreviewRequest {
        file_name: "positions.csv".to_string(),
        content: sample_import_content(),
        content_encoding: None,
    })
    .expect("preview");
    let mut duplicate = preview.draft_rows[0].clone();
    duplicate.account = Some("Second account".to_string());

    let result = portfolio::commit_draft_rows(
        &pool,
        Arc::new(MockMarketDataProvider),
        PortfolioDraftCommitRequest {
            rows: vec![preview.draft_rows[0].clone(), duplicate],
        },
    )
    .await
    .expect("duplicate symbols merge");

    assert_eq!(result.imported_count, 1);
    assert_eq!(result.positions.len(), 1);
    assert_eq!(result.positions[0].symbol, "AAPL");
    assert_eq!(result.positions[0].quantity, 4.0);
    assert_eq!(result.positions[0].average_cost, 100.0);
    assert_eq!(result.positions[0].market_value, 500.0);
    assert_eq!(
        result.positions[0].account.as_deref(),
        Some("Second account")
    );
}

#[tokio::test]
async fn portfolio_draft_commit_merges_without_deleting_existing_positions() {
    let pool = test_pool().await;
    let preview = portfolio::preview(PortfolioImportPreviewRequest {
        file_name: "positions.csv".to_string(),
        content: sample_import_content(),
        content_encoding: None,
    })
    .expect("preview");
    portfolio::commit_draft_rows(
        &pool,
        Arc::new(MockMarketDataProvider),
        PortfolioDraftCommitRequest {
            rows: preview.draft_rows,
        },
    )
    .await
    .expect("first commit");

    let update = portfolio::preview(PortfolioImportPreviewRequest {
        file_name: "positions.csv".to_string(),
        content: [
            "symbol,name,quantity,average cost,currency,sector,market value",
            "AAPL,Apple Inc.,3,110,USD,Technology,390",
        ]
        .join("\n"),
        content_encoding: None,
    })
    .expect("update preview");
    portfolio::commit_draft_rows(
        &pool,
        Arc::new(MockMarketDataProvider),
        PortfolioDraftCommitRequest {
            rows: update.draft_rows,
        },
    )
    .await
    .expect("merge commit");

    let positions = portfolio::list_positions(&pool).await.expect("positions");
    assert_eq!(positions.len(), 2);
    assert!(positions.iter().any(|position| position.symbol == "MSFT"));
    let apple = positions
        .iter()
        .find(|position| position.symbol == "AAPL")
        .expect("AAPL");
    assert_eq!(apple.quantity, 3.0);
    assert_eq!(apple.average_cost, 110.0);
}

#[tokio::test]
async fn portfolio_positions_can_be_edited_and_deleted() {
    let pool = test_pool().await;
    let preview = portfolio::preview(PortfolioImportPreviewRequest {
        file_name: "positions.csv".to_string(),
        content: sample_import_content(),
        content_encoding: None,
    })
    .expect("preview");
    portfolio::commit_draft_rows(
        &pool,
        Arc::new(MockMarketDataProvider),
        PortfolioDraftCommitRequest {
            rows: preview.draft_rows,
        },
    )
    .await
    .expect("commit");

    let updated = portfolio::update_position(
        &pool,
        Arc::new(MockMarketDataProvider),
        "AAPL",
        UpdatePortfolioPositionRequest {
            name: Some("Apple Inc.".to_string()),
            quantity: Some(4.0),
            average_cost: Some(120.0),
            currency: Some("USD".to_string()),
            market: Some("US".to_string()),
            sector: Some("Consumer Technology".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect("update");
    assert_eq!(updated.name, "Apple Inc.");
    assert_eq!(updated.quantity, 4.0);
    assert_eq!(updated.sector.as_deref(), Some("Consumer Technology"));

    portfolio::delete_position(&pool, Arc::new(MockMarketDataProvider), "MSFT")
        .await
        .expect("delete");
    let positions = portfolio::list_positions(&pool).await.expect("positions");
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].symbol, "AAPL");
}

#[tokio::test]
async fn portfolio_summary_uses_cny_base_totals_and_marks_stale_fx_fallback() {
    let pool = test_pool().await;
    let preview = portfolio::preview(PortfolioImportPreviewRequest {
        file_name: "positions.csv".to_string(),
        content: [
            "symbol,name,quantity,average cost,currency,market,market value",
            "AAPL,Apple,1,100,USD,US,100",
            "0700.HK,Tencent,10,30,HKD,HK,300",
            "600519,Maotai,1,1000,CNY,CN,1000",
        ]
        .join("\n"),
        content_encoding: None,
    })
    .expect("preview");
    portfolio::commit_draft_rows(
        &pool,
        Arc::new(FxProvider { fail: false }),
        PortfolioDraftCommitRequest {
            rows: preview.draft_rows,
        },
    )
    .await
    .expect("commit");

    let summary = portfolio::summary_with_fx(&pool, Arc::new(FxProvider { fail: false }))
        .await
        .expect("summary");
    assert_eq!(summary.base_currency, "CNY");
    assert_eq!(summary.market_groups.len(), 3);
    assert_eq!(summary.fx_stale_count, 0);
    assert_eq!(
        summary.total_market_value_base,
        100.0 * 7.0 + 300.0 * 0.9 + 1000.0
    );

    let stale_summary = portfolio::summary_with_fx(&pool, Arc::new(FxProvider { fail: true }))
        .await
        .expect("stale summary");
    assert_eq!(stale_summary.fx_stale_count, 2);
    assert_eq!(
        stale_summary.total_market_value_base,
        summary.total_market_value_base
    );
}

#[tokio::test]
async fn portfolio_positions_sort_market_value_by_cny_base_value() {
    let pool = test_pool().await;
    let preview = portfolio::preview(PortfolioImportPreviewRequest {
        file_name: "positions.csv".to_string(),
        content: [
            "symbol,name,quantity,average cost,currency,market,market value",
            "AAPL,Apple,1,100,USD,US,100",
            "600519,Maotai,1,500,CNY,CN,500",
        ]
        .join("\n"),
        content_encoding: None,
    })
    .expect("preview");
    portfolio::commit_draft_rows(
        &pool,
        Arc::new(FxProvider { fail: false }),
        PortfolioDraftCommitRequest {
            rows: preview.draft_rows,
        },
    )
    .await
    .expect("commit");

    let positions = portfolio::list_positions(&pool).await.expect("positions");

    assert_eq!(positions[0].symbol, "AAPL");
    assert_eq!(positions[0].market_value, 100.0);
    assert_eq!(positions[0].market_value_base, 700.0);
    assert_eq!(positions[1].symbol, "600519");
    assert_eq!(positions[1].market_value, 500.0);
    assert_eq!(positions[1].market_value_base, 500.0);
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
            decision_date: None,
            quantity: None,
            notional: None,
            price: None,
            currency: None,
            baseline_type: None,
            hypothetical_notional: None,
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
        Arc::new(MockMarketDataProvider),
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
            provider_chain: vec![AiProviderKind::Mock],
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
        Arc::new(MockMarketDataProvider),
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
            provider_chain: vec![AiProviderKind::Mock],
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
async fn mock_ai_memo_chat_returns_conversational_response() {
    let provider = prudentia_backend::ai::mock::MockAiProvider;
    let response = prudentia_backend::ai::AiProvider::respond_to_memo_chat(
        &provider,
        &prudentia_backend::ai::MemoChatContext {
            thread_title: "腾讯讨论".to_string(),
            thread_summary: String::new(),
            user_message: "你怎么看腾讯广告复苏".to_string(),
            recent_messages: vec![prudentia_backend::ai::MemoChatHistoryMessage {
                role: "user".to_string(),
                content: "你怎么看腾讯广告复苏".to_string(),
            }],
            portfolio_summary: empty_portfolio_summary(),
            portfolio_positions: Vec::new(),
        },
        Locale::Zh,
    )
    .await
    .expect("mock memo chat");

    assert!(response.contains("我听到了"));
    assert!(!response.contains("备忘录草稿"));
    assert!(!response.contains("核心假设"));
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
            provider_chain: vec![AiProviderKind::Mock],
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
        market_groups: Vec::new(),
        base_currency: "CNY".to_string(),
        total_market_value_base: 0.0,
        total_cost_base: 0.0,
        total_unrealized_pnl_base: 0.0,
        fx_rates: Vec::new(),
        fx_stale_count: 0,
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
        market_value_base: 100.0,
        unrealized_pnl: 0.0,
        unrealized_pnl_pct: Some(0.0),
        period_profit_loss_base: None,
        period_return_pct: None,
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

    async fn exchange_rate(
        &self,
        from_currency: &str,
        to_currency: &str,
    ) -> Result<ExchangeRate, MarketDataError> {
        Err(MarketDataError::Provider(format!(
            "{from_currency}/{to_currency} unavailable"
        )))
    }
}

async fn quantified_decision(
    pool: &SqlitePool,
    action: &str,
    symbol: &str,
    quantity: Option<f64>,
    hypothetical_notional: Option<f64>,
    price: f64,
) -> decision::Decision {
    decision::create(
        pool,
        CreateDecisionRequest {
            memo_id: None,
            symbol: Some(symbol.to_string()),
            action: action.to_string(),
            rationale: format!("{action} {symbol} for delta tracking."),
            confidence: 0.7,
            expected_outcome: "Track decision delta.".to_string(),
            review_date: Some("2026-09-30".to_string()),
            decision_date: Some("2026-01-01".to_string()),
            quantity,
            notional: quantity.map(|value| value * price),
            price: Some(price),
            currency: Some("CNY".to_string()),
            baseline_type: None,
            hypothetical_notional,
        },
    )
    .await
    .expect("create quantified decision")
}

struct StaticCnyProvider {
    price: f64,
    fail: bool,
}

#[async_trait]
impl MarketDataProvider for StaticCnyProvider {
    async fn quote(&self, symbol: &str) -> Result<MarketQuote, MarketDataError> {
        if self.fail {
            return Err(MarketDataError::Provider(format!("{symbol} unavailable")));
        }

        Ok(MarketQuote {
            symbol: symbol.to_uppercase(),
            price: self.price,
            currency: Some("CNY".to_string()),
            volume: None,
            source: "static-test".to_string(),
            updated_at: "2026-01-02T00:00:00Z".to_string(),
        })
    }

    async fn exchange_rate(
        &self,
        from_currency: &str,
        to_currency: &str,
    ) -> Result<ExchangeRate, MarketDataError> {
        if self.fail && !from_currency.eq_ignore_ascii_case(to_currency) {
            return Err(MarketDataError::Provider("fx unavailable".to_string()));
        }

        Ok(ExchangeRate {
            from_currency: from_currency.to_ascii_uppercase(),
            to_currency: to_currency.to_ascii_uppercase(),
            rate: 1.0,
            source: "static-test".to_string(),
            updated_at: "2026-01-02T00:00:00Z".to_string(),
        })
    }
}

struct StaticFxQuoteProvider {
    price: f64,
    currency: &'static str,
    fx_rate: f64,
}

#[async_trait]
impl MarketDataProvider for StaticFxQuoteProvider {
    async fn quote(&self, symbol: &str) -> Result<MarketQuote, MarketDataError> {
        Ok(MarketQuote {
            symbol: symbol.to_uppercase(),
            price: self.price,
            currency: Some(self.currency.to_string()),
            volume: None,
            source: "static-fx-test".to_string(),
            updated_at: "2026-01-02T00:00:00Z".to_string(),
        })
    }

    async fn exchange_rate(
        &self,
        from_currency: &str,
        to_currency: &str,
    ) -> Result<ExchangeRate, MarketDataError> {
        Ok(ExchangeRate {
            from_currency: from_currency.to_ascii_uppercase(),
            to_currency: to_currency.to_ascii_uppercase(),
            rate: if from_currency.eq_ignore_ascii_case(to_currency) {
                1.0
            } else {
                self.fx_rate
            },
            source: "static-fx-test".to_string(),
            updated_at: "2026-01-02T00:00:00Z".to_string(),
        })
    }
}

struct CountingFxProvider {
    quote_calls: AtomicUsize,
    fx_calls: AtomicUsize,
}

impl CountingFxProvider {
    fn new() -> Self {
        Self {
            quote_calls: AtomicUsize::new(0),
            fx_calls: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl MarketDataProvider for CountingFxProvider {
    async fn quote(&self, symbol: &str) -> Result<MarketQuote, MarketDataError> {
        self.quote_calls.fetch_add(1, Ordering::SeqCst);
        Ok(MarketQuote {
            symbol: symbol.to_uppercase(),
            price: 120.0,
            currency: Some("USD".to_string()),
            volume: None,
            source: "counting-test".to_string(),
            updated_at: "2026-01-02T00:00:00Z".to_string(),
        })
    }

    async fn exchange_rate(
        &self,
        from_currency: &str,
        to_currency: &str,
    ) -> Result<ExchangeRate, MarketDataError> {
        if !from_currency.eq_ignore_ascii_case(to_currency) {
            self.fx_calls.fetch_add(1, Ordering::SeqCst);
        }
        Ok(ExchangeRate {
            from_currency: from_currency.to_ascii_uppercase(),
            to_currency: to_currency.to_ascii_uppercase(),
            rate: if from_currency.eq_ignore_ascii_case(to_currency) {
                1.0
            } else {
                7.0
            },
            source: "counting-test".to_string(),
            updated_at: "2026-01-02T00:00:00Z".to_string(),
        })
    }
}

struct FxProvider {
    fail: bool,
}

#[async_trait]
impl MarketDataProvider for FxProvider {
    async fn quote(&self, symbol: &str) -> Result<MarketQuote, MarketDataError> {
        Ok(MarketQuote {
            symbol: symbol.to_uppercase(),
            price: 100.0,
            currency: Some("USD".to_string()),
            volume: None,
            source: "test".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        })
    }

    async fn exchange_rate(
        &self,
        from_currency: &str,
        to_currency: &str,
    ) -> Result<ExchangeRate, MarketDataError> {
        if self.fail && !from_currency.eq_ignore_ascii_case(to_currency) {
            return Err(MarketDataError::Provider("fx unavailable".to_string()));
        }

        let rate = match (
            from_currency.to_ascii_uppercase().as_str(),
            to_currency.to_ascii_uppercase().as_str(),
        ) {
            (from, to) if from == to => 1.0,
            ("USD", "CNY") => 7.0,
            ("HKD", "CNY") => 0.9,
            _ => 1.0,
        };

        Ok(ExchangeRate {
            from_currency: from_currency.to_ascii_uppercase(),
            to_currency: to_currency.to_ascii_uppercase(),
            rate,
            source: "test".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        })
    }
}
