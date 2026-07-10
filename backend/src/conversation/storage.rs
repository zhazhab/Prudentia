use serde_json::{json, Value};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{
    ai::ConversationActionDraft,
    error::{AppError, AppResult},
    locale::Locale,
    memo_thread,
    time::now_iso,
};

use super::{
    company::load_company_view,
    types::{
        ConversationAction, ConversationRun, ConversationThreadDetail, ConversationThreadSummary,
        RunEvent, StartRunRequest, ThreadSubject,
    },
};

mod rows;
use rows::{action_from_row, event_from_row, run_from_row, subject_from_row};

pub async fn create_run(
    pool: &SqlitePool,
    request: &StartRunRequest,
    locale: Locale,
    retry_of_run_id: Option<&str>,
) -> AppResult<(ConversationRun, String)> {
    let content = request.content.trim();
    if content.is_empty() {
        return Err(AppError::bad_request("message content is required"));
    }
    if request.client_request_id.trim().is_empty() {
        return Err(AppError::bad_request("client_request_id is required"));
    }
    if let Some(existing) = run_by_client_request(pool, &request.client_request_id).await? {
        return Ok((existing.clone(), existing.thread_id));
    }

    let now = now_iso();
    let mut transaction = pool.begin().await?;
    let thread_id = if let Some(thread_id) = request
        .thread_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let exists = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM memo_threads WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(thread_id)
        .fetch_one(&mut *transaction)
        .await?;
        if exists == 0 {
            return Err(AppError::not_found("conversation thread not found"));
        }
        thread_id.to_string()
    } else if let Some(client_thread_id) = request.client_thread_id.as_deref() {
        sqlx::query_scalar::<_, String>(
            "SELECT id FROM memo_threads WHERE client_thread_id = ? AND deleted_at IS NULL LIMIT 1",
        )
        .bind(client_thread_id)
        .fetch_optional(&mut *transaction)
        .await?
        .unwrap_or_else(|| Uuid::new_v4().to_string())
    } else {
        Uuid::new_v4().to_string()
    };

    let thread_exists =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM memo_threads WHERE id = ?")
            .bind(&thread_id)
            .fetch_one(&mut *transaction)
            .await?
            > 0;
    if !thread_exists {
        let title = title_from_content(content, locale);
        sqlx::query(
            r#"INSERT INTO memo_threads (
                id, title, summary, status, linked_symbols_json, tags_json,
                archived_at, deleted_at, client_thread_id, created_at, updated_at, last_message_at
            ) VALUES (?, ?, '', 'active', '[]', '[]', NULL, NULL, ?, ?, ?, ?)"#,
        )
        .bind(&thread_id)
        .bind(title)
        .bind(&request.client_thread_id)
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            r#"INSERT INTO conversation_thread_subjects (
                thread_id, kind, subject_key, label, confidence, updated_at
            ) VALUES (?, 'general', NULL, NULL, 0, ?)"#,
        )
        .bind(&thread_id)
        .bind(&now)
        .execute(&mut *transaction)
        .await?;
    }

    let active_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM conversation_runs WHERE thread_id = ? AND status IN ('queued', 'running')",
    )
    .bind(&thread_id)
    .fetch_one(&mut *transaction)
    .await?;
    if active_count > 0 {
        return Err(AppError::bad_request(
            "this thread already has an active run",
        ));
    }

    let user_message_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"INSERT INTO memo_thread_messages (
            id, thread_id, role, content, status, request_id, duration_ms,
            artifacts_json, sources_json, used_context_json, created_at, updated_at
        ) VALUES (?, ?, 'user', ?, 'completed', ?, NULL, '[]', '[]', '[]', ?, ?)"#,
    )
    .bind(&user_message_id)
    .bind(&thread_id)
    .bind(content)
    .bind(&request.client_request_id)
    .bind(&now)
    .bind(&now)
    .execute(&mut *transaction)
    .await?;

    let run = ConversationRun {
        id: Uuid::new_v4().to_string(),
        client_request_id: request.client_request_id.clone(),
        thread_id: thread_id.clone(),
        user_message_id,
        assistant_message_id: None,
        retry_of_run_id: retry_of_run_id.map(ToOwned::to_owned),
        status: "queued".to_string(),
        phase: "queued".to_string(),
        provider: None,
        error_code: None,
        error_message: None,
        started_at: now.clone(),
        updated_at: now.clone(),
        finished_at: None,
    };
    sqlx::query(
        r#"INSERT INTO conversation_runs (
            id, client_request_id, thread_id, user_message_id, assistant_message_id,
            retry_of_run_id, status, phase, provider, error_code, error_message,
            started_at, updated_at, finished_at
        ) VALUES (?, ?, ?, ?, NULL, ?, ?, ?, NULL, NULL, NULL, ?, ?, NULL)"#,
    )
    .bind(&run.id)
    .bind(&run.client_request_id)
    .bind(&run.thread_id)
    .bind(&run.user_message_id)
    .bind(&run.retry_of_run_id)
    .bind(&run.status)
    .bind(&run.phase)
    .bind(&run.started_at)
    .bind(&run.updated_at)
    .execute(&mut *transaction)
    .await?;

    for attachment_id in &request.attachment_ids {
        let result = sqlx::query(
            r#"INSERT INTO conversation_run_attachments (run_id, attachment_id)
            SELECT ?, id FROM conversation_attachments WHERE id = ?"#,
        )
        .bind(&run.id)
        .bind(attachment_id)
        .execute(&mut *transaction)
        .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::bad_request(format!(
                "attachment {attachment_id} was not found"
            )));
        }
    }
    sqlx::query("UPDATE memo_threads SET updated_at = ?, last_message_at = ? WHERE id = ?")
        .bind(&now)
        .bind(&now)
        .bind(&thread_id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok((run, thread_id))
}

pub async fn append_event(
    pool: &SqlitePool,
    run_id: &str,
    thread_id: &str,
    event_type: &str,
    payload: Value,
) -> AppResult<RunEvent> {
    let created_at = now_iso();
    let result = sqlx::query(
        r#"INSERT INTO conversation_run_events (
            run_id, thread_id, event_type, payload_json, created_at
        ) VALUES (?, ?, ?, ?, ?)"#,
    )
    .bind(run_id)
    .bind(thread_id)
    .bind(event_type)
    .bind(serde_json::to_string(&payload)?)
    .bind(&created_at)
    .execute(pool)
    .await?;
    Ok(RunEvent {
        event_id: result.last_insert_rowid(),
        run_id: run_id.to_string(),
        thread_id: thread_id.to_string(),
        event_type: event_type.to_string(),
        payload,
        created_at,
    })
}

pub async fn replay_events(pool: &SqlitePool, after_event_id: i64) -> AppResult<Vec<RunEvent>> {
    let rows = sqlx::query(
        r#"SELECT event_id, run_id, thread_id, event_type, payload_json, created_at
        FROM conversation_run_events WHERE event_id > ? ORDER BY event_id ASC LIMIT 2000"#,
    )
    .bind(after_event_id.max(0))
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(event_from_row).collect()
}

pub async fn set_run_phase(
    pool: &SqlitePool,
    run_id: &str,
    phase: &str,
    provider: Option<&str>,
) -> AppResult<()> {
    sqlx::query(
        r#"UPDATE conversation_runs SET status = 'running', phase = ?,
                  provider = COALESCE(?, provider), updated_at = ?
        WHERE id = ? AND status IN ('queued', 'running')"#,
    )
    .bind(phase)
    .bind(provider)
    .bind(now_iso())
    .bind(run_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn finish_run(
    pool: &SqlitePool,
    run_id: &str,
    status: &str,
    phase: &str,
    error_code: Option<&str>,
    error_message: Option<&str>,
) -> AppResult<()> {
    let now = now_iso();
    sqlx::query(
        r#"UPDATE conversation_runs SET status = ?, phase = ?, error_code = ?,
                  error_message = ?, updated_at = ?, finished_at = ? WHERE id = ?"#,
    )
    .bind(status)
    .bind(phase)
    .bind(error_code)
    .bind(error_message)
    .bind(&now)
    .bind(&now)
    .bind(run_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn append_assistant_delta(
    pool: &SqlitePool,
    run_id: &str,
    delta: &str,
) -> AppResult<String> {
    let message_id = ensure_assistant_message(pool, run_id).await?;
    sqlx::query(
        "UPDATE memo_thread_messages SET content = content || ?, status = 'streaming', updated_at = ? WHERE id = ?",
    )
    .bind(delta)
    .bind(now_iso())
    .bind(&message_id)
    .execute(pool)
    .await?;
    Ok(message_id)
}

pub async fn complete_assistant_message(
    pool: &SqlitePool,
    run_id: &str,
    content: &str,
    sources: &[Value],
    used_context: &[Value],
) -> AppResult<String> {
    let message_id = ensure_assistant_message(pool, run_id).await?;
    let run = run_by_id(pool, run_id).await?;
    let duration = chrono::DateTime::parse_from_rfc3339(&run.started_at)
        .map(|started| {
            (chrono::Utc::now() - started.with_timezone(&chrono::Utc)).num_milliseconds()
        })
        .ok();
    sqlx::query(
        r#"UPDATE memo_thread_messages SET content = ?, status = 'completed', duration_ms = ?,
                  sources_json = ?, used_context_json = ?, updated_at = ? WHERE id = ?"#,
    )
    .bind(content)
    .bind(duration)
    .bind(serde_json::to_string(sources)?)
    .bind(serde_json::to_string(used_context)?)
    .bind(now_iso())
    .bind(&message_id)
    .execute(pool)
    .await?;
    Ok(message_id)
}

pub async fn mark_assistant_terminal(
    pool: &SqlitePool,
    run_id: &str,
    status: &str,
    content: Option<&str>,
) -> AppResult<()> {
    let run = run_by_id(pool, run_id).await?;
    if let Some(message_id) = run.assistant_message_id {
        sqlx::query(
            "UPDATE memo_thread_messages SET status = ?, content = COALESCE(?, content), updated_at = ? WHERE id = ?",
        )
        .bind(status)
        .bind(content)
        .bind(now_iso())
        .bind(message_id)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn run_by_id(pool: &SqlitePool, run_id: &str) -> AppResult<ConversationRun> {
    sqlx::query(&run_select("WHERE id = ?"))
        .bind(run_id)
        .fetch_optional(pool)
        .await?
        .map(run_from_row)
        .transpose()?
        .ok_or_else(|| AppError::not_found("conversation run not found"))
}

pub async fn active_runs(pool: &SqlitePool) -> AppResult<Vec<ConversationRun>> {
    let rows = sqlx::query(&run_select(
        "WHERE status IN ('queued', 'running') ORDER BY started_at ASC",
    ))
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(run_from_row).collect()
}

pub async fn list_threads(pool: &SqlitePool) -> AppResult<Vec<ConversationThreadSummary>> {
    let threads = memo_thread::list(pool, 50, false).await?;
    let mut result = Vec::with_capacity(threads.len());
    for thread in threads {
        result.push(ConversationThreadSummary {
            subject: thread_subject(pool, &thread.id).await?,
            active_run: active_run_for_thread(pool, &thread.id).await?,
            thread,
        });
    }
    Ok(result)
}

pub async fn thread_summary(
    pool: &SqlitePool,
    thread_id: &str,
) -> AppResult<ConversationThreadSummary> {
    let detail = memo_thread::get_detail(pool, thread_id, 1, None).await?;
    Ok(ConversationThreadSummary {
        subject: thread_subject(pool, thread_id).await?,
        active_run: active_run_for_thread(pool, thread_id).await?,
        thread: detail.thread,
    })
}

pub async fn thread_detail(
    pool: &SqlitePool,
    thread_id: &str,
    message_limit: i64,
    before_message_id: Option<&str>,
) -> AppResult<ConversationThreadDetail> {
    let detail = memo_thread::get_detail(pool, thread_id, message_limit, before_message_id).await?;
    let summary = ConversationThreadSummary {
        subject: thread_subject(pool, thread_id).await?,
        active_run: active_run_for_thread(pool, thread_id).await?,
        thread: detail.thread,
    };
    let company_view = if summary.subject.kind == "company" {
        match summary.subject.subject_key.as_deref() {
            Some(symbol) => load_company_view(pool, symbol).await?,
            None => None,
        }
    } else {
        None
    };
    Ok(ConversationThreadDetail {
        latest_run: latest_run_for_thread(pool, thread_id).await?,
        messages: detail.messages,
        actions: actions_for_thread(pool, thread_id).await?,
        company_view,
        has_more: detail.has_more,
        thread: summary,
    })
}

pub async fn update_thread_subject(
    pool: &SqlitePool,
    thread_id: &str,
    subject: ThreadSubject,
) -> AppResult<ThreadSubject> {
    validate_subject(&subject)?;
    let result = sqlx::query(
        r#"INSERT INTO conversation_thread_subjects (
            thread_id, kind, subject_key, label, confidence, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT(thread_id) DO UPDATE SET kind = excluded.kind,
            subject_key = excluded.subject_key, label = excluded.label,
            confidence = excluded.confidence, updated_at = excluded.updated_at"#,
    )
    .bind(thread_id)
    .bind(&subject.kind)
    .bind(&subject.subject_key)
    .bind(&subject.label)
    .bind(subject.confidence)
    .bind(now_iso())
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::not_found("conversation thread not found"));
    }
    if subject.kind == "company" {
        sqlx::query("UPDATE memo_threads SET linked_symbols_json = ?, updated_at = ? WHERE id = ?")
            .bind(serde_json::to_string(
                &subject.subject_key.iter().collect::<Vec<_>>(),
            )?)
            .bind(now_iso())
            .bind(thread_id)
            .execute(pool)
            .await?;
    }
    Ok(subject)
}

pub async fn thread_subject(pool: &SqlitePool, thread_id: &str) -> AppResult<ThreadSubject> {
    let row = sqlx::query(
        "SELECT kind, subject_key, label, confidence FROM conversation_thread_subjects WHERE thread_id = ?",
    )
    .bind(thread_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(subject_from_row).transpose()?.unwrap_or_default())
}

pub async fn insert_turn_summary(
    pool: &SqlitePool,
    run_id: &str,
    thread_id: &str,
    summary: &str,
) -> AppResult<()> {
    sqlx::query(
        r#"INSERT OR REPLACE INTO conversation_turn_summaries (
            id, run_id, thread_id, summary, created_at
        ) VALUES (COALESCE((SELECT id FROM conversation_turn_summaries WHERE run_id = ?), ?), ?, ?, ?, ?)"#,
    )
    .bind(run_id)
    .bind(Uuid::new_v4().to_string())
    .bind(run_id)
    .bind(thread_id)
    .bind(summary)
    .bind(now_iso())
    .execute(pool)
    .await?;
    sqlx::query("UPDATE memo_threads SET summary = ?, updated_at = ? WHERE id = ?")
        .bind(summary)
        .bind(now_iso())
        .bind(thread_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn insert_source(
    pool: &SqlitePool,
    run_id: &str,
    source: &crate::ai::ConversationResearchSource,
) -> AppResult<Value> {
    let id = Uuid::new_v4().to_string();
    let retrieved_at = now_iso();
    sqlx::query(
        r#"INSERT INTO conversation_sources (
            id, run_id, title, url, snippet, source_tier, retrieved_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&id)
    .bind(run_id)
    .bind(&source.title)
    .bind(&source.url)
    .bind(&source.snippet)
    .bind(&source.source_tier)
    .bind(&retrieved_at)
    .execute(pool)
    .await?;
    Ok(json!({
        "id": id,
        "title": source.title,
        "url": source.url,
        "snippet": source.snippet,
        "source_tier": source.source_tier,
        "retrieved_at": retrieved_at
    }))
}

pub async fn insert_action(
    pool: &SqlitePool,
    run_id: &str,
    thread_id: &str,
    draft: ConversationActionDraft,
    target_version: Option<i64>,
) -> AppResult<ConversationAction> {
    let now = now_iso();
    let action = ConversationAction {
        id: Uuid::new_v4().to_string(),
        run_id: run_id.to_string(),
        thread_id: thread_id.to_string(),
        action_type: draft.action_type,
        title: draft.title,
        rationale: draft.rationale,
        payload: draft.payload,
        result: None,
        target_version,
        status: "proposed".to_string(),
        error: None,
        created_at: now.clone(),
        updated_at: now,
        executed_at: None,
    };
    sqlx::query(
        r#"INSERT INTO conversation_actions (
            id, run_id, thread_id, action_type, title, rationale, payload_json,
            result_json, target_version, status, error, created_at, updated_at, executed_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, NULL, ?, 'proposed', NULL, ?, ?, NULL)"#,
    )
    .bind(&action.id)
    .bind(&action.run_id)
    .bind(&action.thread_id)
    .bind(&action.action_type)
    .bind(&action.title)
    .bind(&action.rationale)
    .bind(serde_json::to_string(&action.payload)?)
    .bind(action.target_version)
    .bind(&action.created_at)
    .bind(&action.updated_at)
    .execute(pool)
    .await?;
    Ok(action)
}

pub async fn action_by_id(pool: &SqlitePool, action_id: &str) -> AppResult<ConversationAction> {
    sqlx::query(&action_select("WHERE id = ?"))
        .bind(action_id)
        .fetch_optional(pool)
        .await?
        .map(action_from_row)
        .transpose()?
        .ok_or_else(|| AppError::not_found("conversation action not found"))
}

pub async fn update_action_payload(
    pool: &SqlitePool,
    action_id: &str,
    payload: Value,
) -> AppResult<ConversationAction> {
    let current = action_by_id(pool, action_id).await?;
    if !matches!(current.status.as_str(), "proposed" | "edited" | "failed") {
        return Err(AppError::bad_request("only pending actions can be edited"));
    }
    sqlx::query(
        "UPDATE conversation_actions SET payload_json = ?, status = 'edited', error = NULL, updated_at = ? WHERE id = ?",
    )
    .bind(serde_json::to_string(&payload)?)
    .bind(now_iso())
    .bind(action_id)
    .execute(pool)
    .await?;
    action_by_id(pool, action_id).await
}

pub async fn complete_action(
    pool: &SqlitePool,
    action_id: &str,
    status: &str,
    result: Option<Value>,
    error: Option<&str>,
) -> AppResult<ConversationAction> {
    let executed_at = (status == "executed").then(now_iso);
    sqlx::query(
        r#"UPDATE conversation_actions SET status = ?, result_json = ?, error = ?,
                  updated_at = ?, executed_at = ? WHERE id = ?"#,
    )
    .bind(status)
    .bind(
        result
            .map(|value| serde_json::to_string(&value))
            .transpose()?,
    )
    .bind(error)
    .bind(now_iso())
    .bind(executed_at)
    .bind(action_id)
    .execute(pool)
    .await?;
    action_by_id(pool, action_id).await
}

async fn ensure_assistant_message(pool: &SqlitePool, run_id: &str) -> AppResult<String> {
    let run = run_by_id(pool, run_id).await?;
    if let Some(message_id) = run.assistant_message_id {
        return Ok(message_id);
    }
    let id = Uuid::new_v4().to_string();
    let now = now_iso();
    sqlx::query(
        r#"INSERT INTO memo_thread_messages (
            id, thread_id, role, content, status, request_id, duration_ms,
            artifacts_json, sources_json, used_context_json, created_at, updated_at
        ) VALUES (?, ?, 'assistant', '', 'streaming', ?, NULL, '[]', '[]', '[]', ?, ?)"#,
    )
    .bind(&id)
    .bind(&run.thread_id)
    .bind(&run.client_request_id)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;
    sqlx::query(
        "UPDATE conversation_runs SET assistant_message_id = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&id)
    .bind(&now)
    .bind(run_id)
    .execute(pool)
    .await?;
    Ok(id)
}

async fn actions_for_thread(
    pool: &SqlitePool,
    thread_id: &str,
) -> AppResult<Vec<ConversationAction>> {
    let rows = sqlx::query(&action_select(
        "WHERE thread_id = ? ORDER BY created_at ASC",
    ))
    .bind(thread_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(action_from_row).collect()
}

async fn active_run_for_thread(
    pool: &SqlitePool,
    thread_id: &str,
) -> AppResult<Option<ConversationRun>> {
    sqlx::query(&run_select(
        "WHERE thread_id = ? AND status IN ('queued', 'running') ORDER BY started_at DESC LIMIT 1",
    ))
    .bind(thread_id)
    .fetch_optional(pool)
    .await?
    .map(run_from_row)
    .transpose()
}

async fn latest_run_for_thread(
    pool: &SqlitePool,
    thread_id: &str,
) -> AppResult<Option<ConversationRun>> {
    sqlx::query(&run_select(
        "WHERE thread_id = ? ORDER BY started_at DESC LIMIT 1",
    ))
    .bind(thread_id)
    .fetch_optional(pool)
    .await?
    .map(run_from_row)
    .transpose()
}

async fn run_by_client_request(
    pool: &SqlitePool,
    client_request_id: &str,
) -> AppResult<Option<ConversationRun>> {
    sqlx::query(&run_select("WHERE client_request_id = ?"))
        .bind(client_request_id)
        .fetch_optional(pool)
        .await?
        .map(run_from_row)
        .transpose()
}

fn run_select(suffix: &str) -> String {
    format!(
        "SELECT id, client_request_id, thread_id, user_message_id, assistant_message_id, retry_of_run_id, status, phase, provider, error_code, error_message, started_at, updated_at, finished_at FROM conversation_runs {suffix}"
    )
}

fn action_select(suffix: &str) -> String {
    format!(
        "SELECT id, run_id, thread_id, action_type, title, rationale, payload_json, result_json, target_version, status, error, created_at, updated_at, executed_at FROM conversation_actions {suffix}"
    )
}

fn validate_subject(subject: &ThreadSubject) -> AppResult<()> {
    if !matches!(
        subject.kind.as_str(),
        "company" | "investment_system" | "psychology" | "general"
    ) {
        return Err(AppError::bad_request("invalid conversation subject kind"));
    }
    if subject.kind == "company"
        && subject
            .subject_key
            .as_deref()
            .unwrap_or_default()
            .is_empty()
    {
        return Err(AppError::bad_request("company subject requires a symbol"));
    }
    Ok(())
}

fn title_from_content(content: &str, locale: Locale) -> String {
    let mut title = content.chars().take(40).collect::<String>();
    if content.chars().count() > 40 {
        title.push_str("...");
    }
    if title.is_empty() {
        if locale.is_zh() {
            "未命名主题".to_string()
        } else {
            "Untitled thread".to_string()
        }
    } else {
        title
    }
}
