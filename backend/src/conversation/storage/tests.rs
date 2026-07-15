use sqlx::sqlite::SqlitePoolOptions;

use crate::{ai::ConversationActionDraft, database, locale::Locale};

use super::{
    append_event, complete_assistant_message, create_run, finish_run, insert_action, thread_detail,
    StartRunRequest,
};

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
    let message_id = complete_assistant_message(&pool, &run.id, "analysis", &[], &[])
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
