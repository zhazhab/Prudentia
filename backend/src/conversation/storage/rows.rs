use sqlx::Row;

use crate::error::AppResult;

use super::super::types::{ConversationAction, ConversationRun, RunEvent, ThreadSubject};

pub(super) fn run_from_row(row: sqlx::sqlite::SqliteRow) -> AppResult<ConversationRun> {
    Ok(ConversationRun {
        id: row.try_get("id")?,
        client_request_id: row.try_get("client_request_id")?,
        thread_id: row.try_get("thread_id")?,
        user_message_id: row.try_get("user_message_id")?,
        assistant_message_id: row.try_get("assistant_message_id")?,
        retry_of_run_id: row.try_get("retry_of_run_id")?,
        status: row.try_get("status")?,
        phase: row.try_get("phase")?,
        provider: row.try_get("provider")?,
        error_code: row.try_get("error_code")?,
        error_message: row.try_get("error_message")?,
        started_at: row.try_get("started_at")?,
        updated_at: row.try_get("updated_at")?,
        finished_at: row.try_get("finished_at")?,
    })
}

pub(super) fn event_from_row(row: sqlx::sqlite::SqliteRow) -> AppResult<RunEvent> {
    Ok(RunEvent {
        event_id: row.try_get("event_id")?,
        run_id: row.try_get("run_id")?,
        thread_id: row.try_get("thread_id")?,
        event_type: row.try_get("event_type")?,
        payload: serde_json::from_str(&row.try_get::<String, _>("payload_json")?)?,
        created_at: row.try_get("created_at")?,
    })
}

pub(super) fn action_from_row(row: sqlx::sqlite::SqliteRow) -> AppResult<ConversationAction> {
    Ok(ConversationAction {
        id: row.try_get("id")?,
        run_id: row.try_get("run_id")?,
        thread_id: row.try_get("thread_id")?,
        action_type: row.try_get("action_type")?,
        title: row.try_get("title")?,
        rationale: row.try_get("rationale")?,
        payload: serde_json::from_str(&row.try_get::<String, _>("payload_json")?)?,
        result: row
            .try_get::<Option<String>, _>("result_json")?
            .map(|value| serde_json::from_str(&value))
            .transpose()?,
        target_version: row.try_get("target_version")?,
        status: row.try_get("status")?,
        error: row.try_get("error")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        executed_at: row.try_get("executed_at")?,
    })
}

pub(super) fn subject_from_row(row: sqlx::sqlite::SqliteRow) -> AppResult<ThreadSubject> {
    Ok(ThreadSubject {
        kind: row.try_get("kind")?,
        subject_key: row.try_get("subject_key")?,
        label: row.try_get("label")?,
        confidence: row.try_get("confidence")?,
    })
}
