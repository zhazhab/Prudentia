use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use serde_json::{json, Value};
use sqlx::SqlitePool;
use tokio::{
    sync::{broadcast, mpsc},
    task::JoinHandle,
};
use uuid::Uuid;

use crate::{
    ai::{runtime::AiRuntime, AiProviderEvent, ConversationActionDraft, ConversationProjection},
    error::{AppError, AppResult},
    locale::Locale,
    market_data::MarketDataProvider,
};

use super::{
    actions,
    context::{assemble_context, resolve_subject, ConversationResearchContext},
    research::{search_with_cache, should_research, WebResearchProvider},
    storage,
    types::{
        ConfirmActionRequest, ConversationAction, ConversationRun, ConversationThreadDetail,
        ConversationThreadSummary, RunEvent, StartRunRequest, StartRunResponse, ThreadSubject,
        UpdateActionRequest, UpdateSubjectRequest,
    },
};

pub struct ConversationEngine {
    pool: SqlitePool,
    ai: Arc<AiRuntime>,
    market_data: Arc<dyn MarketDataProvider>,
    research: Arc<dyn WebResearchProvider>,
    workspace_dir: PathBuf,
    tasks: Mutex<HashMap<String, JoinHandle<()>>>,
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
            storage::mark_assistant_terminal(&self.pool, &run.id, "failed", None).await?;
            storage::finish_run(
                &self.pool,
                &run.id,
                "interrupted",
                "interrupted",
                Some("backend_restarted"),
                Some("The backend restarted before this response completed."),
            )
            .await?;
            self.emit(
                &run.id,
                &run.thread_id,
                "run.interrupted",
                json!({ "code": "backend_restarted", "retryable": true }),
            )
            .await?;
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
            "run.accepted",
            json!({ "run": run, "thread": thread }),
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
        if let Some(handle) = self
            .tasks
            .lock()
            .expect("conversation task lock poisoned")
            .remove(run_id)
        {
            handle.abort();
        }
        storage::mark_assistant_terminal(&self.pool, run_id, "canceled", None).await?;
        storage::finish_run(&self.pool, run_id, "canceled", "canceled", None, None).await?;
        self.emit(
            run_id,
            &run.thread_id,
            "run.canceled",
            json!({ "retryable": true }),
        )
        .await?;
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
            "run.accepted",
            json!({ "run": run, "thread": thread, "retry_of_run_id": run_id }),
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
            "action.updated",
            serde_json::to_value(&updated)?,
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
            "action.updated",
            serde_json::to_value(&action)?,
        )
        .await?;
        Ok(action)
    }

    pub async fn reject_action(&self, action_id: &str) -> AppResult<ConversationAction> {
        let action = actions::reject_action(&self.pool, action_id).await?;
        self.emit(
            &action.run_id,
            &action.thread_id,
            "action.updated",
            serde_json::to_value(&action)?,
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

    fn spawn(self: &Arc<Self>, run_id: String, locale: Locale) {
        let engine = self.clone();
        let task_run_id = run_id.clone();
        let handle = tokio::spawn(async move {
            if let Err(error) = engine.run_turn(&task_run_id, locale).await {
                let _ = engine
                    .fail_run(&task_run_id, "conversation_failed", &error.to_string())
                    .await;
            }
            engine
                .tasks
                .lock()
                .expect("conversation task lock poisoned")
                .remove(&task_run_id);
        });
        self.tasks
            .lock()
            .expect("conversation task lock poisoned")
            .insert(run_id, handle);
    }

    async fn run_turn(&self, run_id: &str, locale: Locale) -> AppResult<()> {
        let run = storage::run_by_id(&self.pool, run_id).await?;
        let user_message = sqlx::query_scalar::<_, String>(
            "SELECT content FROM memo_thread_messages WHERE id = ?",
        )
        .bind(&run.user_message_id)
        .fetch_one(&self.pool)
        .await?;

        self.phase(&run, "resolving_subject", None, None).await?;
        let subject = resolve_subject(&self.pool, &run.thread_id, &user_message).await?;

        let mut sources = Vec::new();
        let mut source_values = Vec::new();
        let mut research_warning = None;
        if should_research(&user_message) {
            self.phase(&run, "researching", None, None).await?;
            match search_with_cache(&self.pool, self.research.clone(), &user_message).await {
                Ok(results) => {
                    for source in results {
                        let value = storage::insert_source(&self.pool, run_id, &source).await?;
                        self.emit(run_id, &run.thread_id, "source.added", value.clone())
                            .await?;
                        source_values.push(value);
                        sources.push(source);
                    }
                }
                Err(error) => research_warning = Some(error.to_string()),
            }
        }

        self.phase(&run, "loading_context", None, None).await?;
        let context = assemble_context(
            &self.pool,
            &self.workspace_dir,
            run_id,
            &run.thread_id,
            &user_message,
            &subject,
            ConversationResearchContext {
                sources,
                warning: research_warning,
            },
        )
        .await?;

        self.phase(&run, "generating", None, None).await?;
        let (provider_tx, mut provider_rx) = mpsc::unbounded_channel();
        let response = self
            .ai
            .respond_to_conversation(&context, locale, provider_tx);
        tokio::pin!(response);
        let mut saw_delta = false;
        let assistant_response = loop {
            tokio::select! {
                event = provider_rx.recv() => {
                    let Some(event) = event else {
                        break response
                            .await
                            .map_err(|error| AppError::internal(error.to_string()))?;
                    };
                    saw_delta |= self.handle_provider_event(&run, event).await?;
                }
                result = &mut response => {
                    break result.map_err(|error| AppError::internal(error.to_string()))?;
                },
            }
        };
        while let Ok(event) = provider_rx.try_recv() {
            saw_delta |= self.handle_provider_event(&run, event).await?;
        }

        let source_json = source_values;
        let used_context = context.used_context.clone();
        let message_id = storage::complete_assistant_message(
            &self.pool,
            run_id,
            &assistant_response,
            &source_json,
            &used_context,
        )
        .await?;
        if !saw_delta {
            self.emit(
                run_id,
                &run.thread_id,
                "message.completed",
                json!({ "message_id": message_id, "content": assistant_response }),
            )
            .await?;
        } else {
            self.emit(
                run_id,
                &run.thread_id,
                "message.completed",
                json!({ "message_id": message_id }),
            )
            .await?;
        }

        let projection = if should_skip_action_projection(
            &user_message,
            !context.attachments.is_empty(),
            !context.research_sources.is_empty(),
        ) {
            ConversationProjection {
                summary: casual_turn_summary(locale).to_string(),
                actions: Vec::new(),
            }
        } else {
            self.phase(&run, "extracting_actions", None, None).await?;
            match self
                .ai
                .project_conversation(&context, &assistant_response, locale)
                .await
            {
                Ok(projection) => projection,
                Err(error) => {
                    self.emit(
                        run_id,
                        &run.thread_id,
                        "run.warning",
                        json!({ "code": "action_projection_failed", "message": error.to_string() }),
                    )
                    .await?;
                    ConversationProjection {
                        summary: fallback_summary(&user_message),
                        actions: Vec::new(),
                    }
                }
            }
        };

        self.phase(&run, "persisting", None, None).await?;
        storage::insert_turn_summary(&self.pool, run_id, &run.thread_id, &projection.summary)
            .await?;
        for mut draft in projection.actions {
            enrich_draft(&mut draft, &subject);
            match actions::prepare_action(&self.pool, self.market_data.clone(), draft).await {
                Ok((draft, target_version)) => {
                    let action = storage::insert_action(
                        &self.pool,
                        run_id,
                        &run.thread_id,
                        draft,
                        target_version,
                    )
                    .await?;
                    self.emit(
                        run_id,
                        &run.thread_id,
                        "action.proposed",
                        serde_json::to_value(&action)?,
                    )
                    .await?;
                }
                Err(error) => {
                    self.emit(
                        run_id,
                        &run.thread_id,
                        "run.warning",
                        json!({ "code": "invalid_action_proposal", "message": error.to_string() }),
                    )
                    .await?;
                }
            }
        }
        storage::finish_run(&self.pool, run_id, "completed", "completed", None, None).await?;
        self.emit(
            run_id,
            &run.thread_id,
            "run.completed",
            json!({ "message_id": message_id }),
        )
        .await?;
        Ok(())
    }

    async fn handle_provider_event(
        &self,
        run: &ConversationRun,
        event: AiProviderEvent,
    ) -> AppResult<bool> {
        match event {
            AiProviderEvent::Stage { provider, stage } => {
                storage::set_run_phase(&self.pool, &run.id, "generating", Some(&provider)).await?;
                self.emit(
                    &run.id,
                    &run.thread_id,
                    "run.phase",
                    json!({ "phase": "generating", "provider": provider, "provider_stage": stage }),
                )
                .await?;
                Ok(false)
            }
            AiProviderEvent::TextDelta(content) => {
                let message_id =
                    storage::append_assistant_delta(&self.pool, &run.id, &content).await?;
                self.emit(
                    &run.id,
                    &run.thread_id,
                    "message.delta",
                    json!({ "message_id": message_id, "content": content }),
                )
                .await?;
                Ok(true)
            }
        }
    }

    async fn phase(
        &self,
        run: &ConversationRun,
        phase: &str,
        provider: Option<&str>,
        detail: Option<Value>,
    ) -> AppResult<()> {
        storage::set_run_phase(&self.pool, &run.id, phase, provider).await?;
        self.emit(
            &run.id,
            &run.thread_id,
            "run.phase",
            json!({ "phase": phase, "provider": provider, "detail": detail }),
        )
        .await?;
        Ok(())
    }

    async fn fail_run(&self, run_id: &str, code: &str, message: &str) -> AppResult<()> {
        let run = storage::run_by_id(&self.pool, run_id).await?;
        if !matches!(run.status.as_str(), "queued" | "running") {
            return Ok(());
        }
        storage::mark_assistant_terminal(&self.pool, run_id, "failed", None).await?;
        storage::finish_run(
            &self.pool,
            run_id,
            "failed",
            "failed",
            Some(code),
            Some(message),
        )
        .await?;
        self.emit(
            run_id,
            &run.thread_id,
            "run.failed",
            json!({ "code": code, "message": message, "retryable": true }),
        )
        .await?;
        Ok(())
    }

    async fn emit(
        &self,
        run_id: &str,
        thread_id: &str,
        event_type: &str,
        payload: Value,
    ) -> AppResult<RunEvent> {
        let event =
            storage::append_event(&self.pool, run_id, thread_id, event_type, payload).await?;
        let _ = self.events.send(event.clone());
        Ok(event)
    }
}

fn enrich_draft(draft: &mut ConversationActionDraft, subject: &ThreadSubject) {
    let Some(object) = draft.payload.as_object_mut() else {
        return;
    };
    let should_enrich_symbol = matches!(
        draft.action_type.as_str(),
        "company_view_patch" | "trade_record"
    ) && !object.contains_key("symbol");
    if let (true, Some(symbol)) = (should_enrich_symbol, subject.subject_key.as_ref()) {
        object.insert("symbol".to_string(), Value::String(symbol.clone()));
    }
    if draft.action_type == "company_view_patch" && !object.contains_key("company_name") {
        if let Some(label) = &subject.label {
            object.insert("company_name".to_string(), Value::String(label.clone()));
        }
    }
}

fn fallback_summary(message: &str) -> String {
    let mut summary = message.trim().chars().take(240).collect::<String>();
    if message.chars().count() > 240 {
        summary.push_str("...");
    }
    summary
}

fn should_skip_action_projection(
    message: &str,
    has_attachments: bool,
    has_research_sources: bool,
) -> bool {
    if has_attachments || has_research_sources {
        return false;
    }

    let normalized = message
        .trim()
        .trim_matches(|character: char| {
            character.is_whitespace()
                || matches!(
                    character,
                    '!' | '！'
                        | '?'
                        | '？'
                        | '.'
                        | '。'
                        | ','
                        | '，'
                        | ':'
                        | '：'
                        | ';'
                        | '；'
                        | '~'
                        | '～'
                )
        })
        .to_ascii_lowercase();

    matches!(
        normalized.as_str(),
        "你好"
            | "您好"
            | "你好啊"
            | "嗨"
            | "在吗"
            | "早上好"
            | "下午好"
            | "晚上好"
            | "晚安"
            | "你是谁"
            | "你能做什么"
            | "你可以做什么"
            | "你能干什么"
            | "能干什么"
            | "hello"
            | "hi"
            | "hey"
            | "who are you"
            | "what can you do"
    )
}

fn casual_turn_summary(locale: Locale) -> &'static str {
    if locale.is_zh() {
        "用户进行了寒暄或能力询问，未产生可确认变更。"
    } else {
        "The user greeted the assistant or asked about its capabilities; no confirmable changes were proposed."
    }
}

#[cfg(test)]
mod tests {
    use super::should_skip_action_projection;

    #[test]
    fn casual_turns_skip_action_projection() {
        assert!(should_skip_action_projection("你好！", false, false));
        assert!(should_skip_action_projection(
            "What can you do?",
            false,
            false
        ));
    }

    #[test]
    fn material_or_evidence_backed_turns_keep_action_projection() {
        assert!(!should_skip_action_projection(
            "你好，帮我记录买入 100 股。",
            false,
            false
        ));
        assert!(!should_skip_action_projection("你好", true, false));
        assert!(!should_skip_action_projection("你好", false, true));
    }
}
