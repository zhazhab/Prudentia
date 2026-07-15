use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use sqlx::SqlitePool;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::{
    ai::runtime::AiRuntime,
    error::{AppError, AppResult},
    locale::Locale,
    market_data::MarketDataProvider,
};

use super::{
    actions,
    research::WebResearchProvider,
    storage,
    types::{
        ConfirmActionRequest, ConversationAction, ConversationRun, ConversationThreadDetail,
        ConversationThreadSummary, RunEvent, StartRunRequest, StartRunResponse, ThreadSubject,
        UpdateActionRequest, UpdateSubjectRequest,
    },
};

mod events;
mod runtime;
mod subject_clarification;
mod task;
mod turn_context;
mod turn_support;

use events::ConversationEvent;
use task::TurnTask;

pub struct ConversationEngine {
    pool: SqlitePool,
    ai: Arc<AiRuntime>,
    market_data: Arc<dyn MarketDataProvider>,
    research: Arc<dyn WebResearchProvider>,
    workspace_dir: PathBuf,
    tasks: Mutex<HashMap<String, TurnTask>>,
    events: broadcast::Sender<RunEvent>,
}

impl ConversationEngine {
    pub fn new(
        pool: SqlitePool,
        ai: Arc<AiRuntime>,
        market_data: Arc<dyn MarketDataProvider>,
        research: Arc<dyn WebResearchProvider>,
        workspace_dir: PathBuf,
    ) -> Arc<Self> {
        let (events, _) = broadcast::channel(2048);
        Arc::new(Self {
            pool,
            ai,
            market_data,
            research,
            workspace_dir,
            tasks: Mutex::new(HashMap::new()),
            events,
        })
    }

    pub async fn recover_interrupted(&self) -> AppResult<()> {
        for run in storage::active_runs(&self.pool).await? {
            let transitioned = storage::finish_run(
                &self.pool,
                &run.id,
                "interrupted",
                "interrupted",
                Some("backend_restarted"),
                Some("The backend restarted before this response completed."),
            )
            .await?;
            if transitioned {
                storage::mark_assistant_terminal(&self.pool, &run.id, "failed", None).await?;
                self.emit(
                    &run.id,
                    &run.thread_id,
                    ConversationEvent::RunInterrupted {
                        code: "backend_restarted".to_string(),
                        retryable: true,
                    },
                )
                .await?;
            }
        }
        Ok(())
    }

    pub async fn start_run(
        self: &Arc<Self>,
        request: StartRunRequest,
    ) -> AppResult<StartRunResponse> {
        let locale = request
            .locale
            .as_deref()
            .map(Locale::from_accept_language)
            .unwrap_or(Locale::En);
        let (run, thread_id) = storage::create_run(&self.pool, &request, locale, None).await?;
        let thread = storage::thread_summary(&self.pool, &thread_id).await?;
        self.emit(
            &run.id,
            &run.thread_id,
            ConversationEvent::RunAccepted {
                run: run.clone(),
                thread: thread.clone(),
            },
        )
        .await?;
        if run.status == "queued" {
            self.spawn(run.id.clone(), locale);
        }
        Ok(StartRunResponse { run, thread })
    }

    pub async fn cancel_run(&self, run_id: &str) -> AppResult<ConversationRun> {
        let run = storage::run_by_id(&self.pool, run_id).await?;
        if !matches!(run.status.as_str(), "queued" | "running") {
            return Ok(run);
        }
        let task = self
            .tasks
            .lock()
            .expect("conversation task lock poisoned")
            .remove(run_id);
        if let Some(task) = task {
            task.cancel_and_wait().await;
        }
        let transitioned =
            storage::finish_run(&self.pool, run_id, "canceled", "canceled", None, None).await?;
        if transitioned {
            storage::mark_assistant_terminal(&self.pool, run_id, "canceled", None).await?;
            self.emit(
                run_id,
                &run.thread_id,
                ConversationEvent::RunCanceled { retryable: true },
            )
            .await?;
        }
        storage::run_by_id(&self.pool, run_id).await
    }

    pub async fn retry_run(self: &Arc<Self>, run_id: &str) -> AppResult<StartRunResponse> {
        let previous = storage::run_by_id(&self.pool, run_id).await?;
        if matches!(previous.status.as_str(), "queued" | "running") {
            return Err(AppError::bad_request("an active run cannot be retried"));
        }
        let content = sqlx::query_scalar::<_, String>(
            "SELECT content FROM memo_thread_messages WHERE id = ?",
        )
        .bind(&previous.user_message_id)
        .fetch_one(&self.pool)
        .await?;
        let attachment_ids = sqlx::query_scalar::<_, String>(
            "SELECT attachment_id FROM conversation_run_attachments WHERE run_id = ?",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?;
        let request = StartRunRequest {
            client_request_id: format!("retry:{}", Uuid::new_v4()),
            thread_id: Some(previous.thread_id.clone()),
            client_thread_id: None,
            content,
            attachment_ids,
            locale: Some("zh-CN".to_string()),
        };
        let locale = Locale::Zh;
        let (run, thread_id) =
            storage::create_run(&self.pool, &request, locale, Some(run_id)).await?;
        let thread = storage::thread_summary(&self.pool, &thread_id).await?;
        self.emit(
            &run.id,
            &thread_id,
            ConversationEvent::RunRetried {
                run: run.clone(),
                thread: thread.clone(),
                retry_of_run_id: run_id.to_string(),
            },
        )
        .await?;
        self.spawn(run.id.clone(), locale);
        Ok(StartRunResponse { run, thread })
    }
    pub async fn list_threads(&self) -> AppResult<Vec<ConversationThreadSummary>> {
        storage::list_threads(&self.pool).await
    }

    pub async fn thread_detail(
        &self,
        thread_id: &str,
        limit: i64,
        before: Option<&str>,
    ) -> AppResult<ConversationThreadDetail> {
        storage::thread_detail(&self.pool, thread_id, limit, before).await
    }

    pub async fn active_runs(&self) -> AppResult<Vec<ConversationRun>> {
        storage::active_runs(&self.pool).await
    }

    pub async fn update_subject(
        &self,
        thread_id: &str,
        request: UpdateSubjectRequest,
    ) -> AppResult<ThreadSubject> {
        storage::update_thread_subject(
            &self.pool,
            thread_id,
            ThreadSubject {
                kind: request.kind,
                subject_key: request.subject_key.map(|value| value.to_ascii_uppercase()),
                label: request.label,
                confidence: 1.0,
            },
        )
        .await
    }

    pub async fn update_action(
        &self,
        action_id: &str,
        request: UpdateActionRequest,
    ) -> AppResult<ConversationAction> {
        let action = storage::action_by_id(&self.pool, action_id).await?;
        actions::validate_edited_payload(&action.action_type, &request.payload)?;
        let updated =
            storage::update_action_payload(&self.pool, action_id, request.payload).await?;
        self.emit(
            &updated.run_id,
            &updated.thread_id,
            ConversationEvent::ActionUpdated(updated.clone()),
        )
        .await?;
        Ok(updated)
    }

    pub async fn confirm_action(
        &self,
        action_id: &str,
        request: ConfirmActionRequest,
    ) -> AppResult<ConversationAction> {
        let action = actions::execute_action(
            &self.pool,
            &self.workspace_dir,
            self.market_data.clone(),
            action_id,
            request.expected_version,
        )
        .await?;
        self.emit(
            &action.run_id,
            &action.thread_id,
            ConversationEvent::ActionUpdated(action.clone()),
        )
        .await?;
        Ok(action)
    }

    pub async fn reject_action(&self, action_id: &str) -> AppResult<ConversationAction> {
        let action = actions::reject_action(&self.pool, action_id).await?;
        self.emit(
            &action.run_id,
            &action.thread_id,
            ConversationEvent::ActionUpdated(action.clone()),
        )
        .await?;
        Ok(action)
    }

    pub async fn replay_events(&self, after: i64) -> AppResult<Vec<RunEvent>> {
        storage::replay_events(&self.pool, after).await
    }
    pub fn subscribe(&self) -> broadcast::Receiver<RunEvent> {
        self.events.subscribe()
    }
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
    pub fn workspace_dir(&self) -> &PathBuf {
        &self.workspace_dir
    }
}

#[cfg(test)]
mod tests;
