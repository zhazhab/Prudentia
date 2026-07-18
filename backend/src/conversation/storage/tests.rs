use sqlx::sqlite::SqlitePoolOptions;

use crate::{
    ai::{ConversationActionDraft, ConversationResearchSource},
    database,
    locale::Locale,
};

use super::{
    active_runs, append_event, complete_assistant_message, create_run, finish_run, insert_action,
    insert_source, thread_detail, StartRunRequest,
};

#[tokio::test]
async fn sources_are_deduplicated_by_url_within_one_run() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("connect sqlite");
    database::migrate(&pool).await.expect("migrate");
    let request = StartRunRequest {
        client_request_id: "source-dedup-test".to_string(),
        thread_id: None,
        client_thread_id: Some("source-dedup-thread".to_string()),
        content: "analyze the company".to_string(),
        attachment_ids: Vec::new(),
        locale: Some("en-US".to_string()),
    };
    let (run, _) = create_run(&pool, &request, Locale::En, None)
        .await
        .expect("create run");
    let source = ConversationResearchSource {
        title: "Primary source".to_string(),
        url: "https://example.com/filing".to_string(),
        snippet: "bounded evidence".to_string(),
        source_tier: "primary".to_string(),
    };

    let (first, first_inserted) = insert_source(&pool, &run.id, &source)
        .await
        .expect("insert first source");
    let (second, second_inserted) = insert_source(&pool, &run.id, &source)
        .await
        .expect("reuse source");

    assert!(first_inserted);
    assert!(!second_inserted);
    assert_eq!(first.id, second.id);
    let count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM conversation_sources WHERE run_id = ?")
            .bind(&run.id)
            .fetch_one(&pool)
            .await
            .expect("count sources");
    assert_eq!(count, 1);
}

#[tokio::test]
async fn active_runs_restore_unfinished_capability_calls() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("connect sqlite");
    database::migrate(&pool).await.expect("migrate");
    let request = StartRunRequest {
        client_request_id: "active-capability-test".to_string(),
        thread_id: None,
        client_thread_id: Some("active-capability-thread".to_string()),
        content: "audit the moat".to_string(),
        attachment_ids: Vec::new(),
        locale: Some("en-US".to_string()),
    };
    let (run, thread_id) = create_run(&pool, &request, Locale::En, None)
        .await
        .expect("create run");
    let payload = serde_json::json!({
        "call_id": "call-1",
        "tool_name": "audit_moat",
        "tool_version": 1,
        "capability_kind": "skill",
        "display_name": "Moat audit",
        "stage": "analysis",
        "activity": "skill_auditing_moat",
        "subject_label": "Example",
        "step_index": 1,
        "total_steps": 1
    });
    append_event(&pool, &run.id, &thread_id, "tool.started", payload)
        .await
        .expect("append tool start");
    append_event(
        &pool,
        &run.id,
        &thread_id,
        "run.plan.created",
        serde_json::json!({
            "template_id": "company_analysis_v1",
            "scope": "moat",
            "dimensions": ["business_model", "moat"],
            "steps": [
                { "id": "scope", "status": "completed" },
                { "id": "research", "status": "pending" }
            ]
        }),
    )
    .await
    .expect("append run plan");
    append_event(
        &pool,
        &run.id,
        &thread_id,
        "run.plan.step",
        serde_json::json!({ "step_id": "research", "status": "running" }),
    )
    .await
    .expect("append plan progress");

    let active = active_runs(&pool).await.expect("load active runs");
    assert_eq!(active[0].active_capabilities.len(), 1);
    assert_eq!(active[0].active_capabilities[0].call_id, "call-1");
    let plan = active[0].execution_plan.as_ref().expect("active run plan");
    assert_eq!(plan.scope, "moat");
    assert_eq!(plan.steps[1].status, "running");

    append_event(
        &pool,
        &run.id,
        &thread_id,
        "tool.completed",
        serde_json::json!({ "call_id": "call-1" }),
    )
    .await
    .expect("append tool completion");
    assert!(active_runs(&pool).await.expect("reload active runs")[0]
        .active_capabilities
        .is_empty());
}

#[tokio::test]
async fn terminal_run_compacts_only_streaming_delta_events() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("connect sqlite");
    database::migrate(&pool).await.expect("migrate");

    let request = StartRunRequest {
        client_request_id: "storage-retention-test".to_string(),
        thread_id: None,
        client_thread_id: Some("storage-retention-thread".to_string()),
        content: "hello".to_string(),
        attachment_ids: Vec::new(),
        locale: Some("en-US".to_string()),
    };
    let (run, thread_id) = create_run(&pool, &request, Locale::En, None)
        .await
        .expect("create run");

    append_event(
        &pool,
        &run.id,
        &thread_id,
        "run.accepted",
        serde_json::json!({ "run_id": run.id }),
    )
    .await
    .expect("append accepted event");
    for delta in ["partial ", "response"] {
        append_event(
            &pool,
            &run.id,
            &thread_id,
            "message.delta",
            serde_json::json!({ "delta": delta }),
        )
        .await
        .expect("append delta event");
    }

    assert!(
        finish_run(&pool, &run.id, "completed", "completed", None, None)
            .await
            .expect("finish run")
    );

    let delta_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM conversation_run_events WHERE run_id = ? AND event_type = 'message.delta'",
    )
    .bind(&run.id)
    .fetch_one(&pool)
    .await
    .expect("count delta events");
    let accepted_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM conversation_run_events WHERE run_id = ? AND event_type = 'run.accepted'",
    )
    .bind(&run.id)
    .fetch_one(&pool)
    .await
    .expect("count accepted events");
    let status =
        sqlx::query_scalar::<_, String>("SELECT status FROM conversation_runs WHERE id = ?")
            .bind(&run.id)
            .fetch_one(&pool)
            .await
            .expect("load run status");

    assert_eq!(delta_count, 0);
    assert_eq!(accepted_count, 1);
    assert_eq!(status, "completed");
}

#[tokio::test]
async fn the_first_terminal_transition_wins() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("connect sqlite");
    database::migrate(&pool).await.expect("migrate");
    let request = StartRunRequest {
        client_request_id: "terminal-transition-test".to_string(),
        thread_id: None,
        client_thread_id: Some("terminal-transition-thread".to_string()),
        content: "hello".to_string(),
        attachment_ids: Vec::new(),
        locale: Some("en-US".to_string()),
    };
    let (run, _) = create_run(&pool, &request, Locale::En, None)
        .await
        .expect("create run");

    assert!(
        finish_run(&pool, &run.id, "completed", "completed", None, None)
            .await
            .expect("complete run")
    );
    assert!(
        !finish_run(&pool, &run.id, "canceled", "canceled", None, None)
            .await
            .expect("reject a second terminal transition")
    );

    let status =
        sqlx::query_scalar::<_, String>("SELECT status FROM conversation_runs WHERE id = ?")
            .bind(&run.id)
            .fetch_one(&pool)
            .await
            .expect("load run status");
    assert_eq!(status, "completed");
}

#[tokio::test]
async fn thread_actions_reference_the_assistant_message_that_proposed_them() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("connect sqlite");
    database::migrate(&pool).await.expect("migrate");

    let request = StartRunRequest {
        client_request_id: "action-message-test".to_string(),
        thread_id: None,
        client_thread_id: Some("action-message-thread".to_string()),
        content: "analyze the company".to_string(),
        attachment_ids: Vec::new(),
        locale: Some("en-US".to_string()),
    };
    let (run, thread_id) = create_run(&pool, &request, Locale::En, None)
        .await
        .expect("create run");
    let message_id = complete_assistant_message(&pool, &run.id, "analysis", &[], &[], &[])
        .await
        .expect("complete assistant message");
    let action = insert_action(
        &pool,
        &run.id,
        &thread_id,
        ConversationActionDraft {
            action_type: "company_view_patch".to_string(),
            title: "Update view".to_string(),
            rationale: "New operating evidence".to_string(),
            payload: serde_json::json!({"symbol": "TEST", "changes": {}}),
        },
        Some(0),
    )
    .await
    .expect("insert action");

    assert_eq!(
        action.assistant_message_id.as_deref(),
        Some(message_id.as_str())
    );
    let detail = thread_detail(&pool, &thread_id, 50, None)
        .await
        .expect("load thread detail");
    assert_eq!(detail.actions.len(), 1);
    assert_eq!(
        detail.actions[0].assistant_message_id.as_deref(),
        Some(message_id.as_str())
    );
}
