use std::time::Duration;

use serde_json::Value;
use tokio::{
    task::{JoinError, JoinHandle},
    time::error::Elapsed,
};

use crate::{
    ai::{runtime::TaskComplexity, AiError, ConversationActionDraft},
    error::{AppError, AppResult},
    locale::Locale,
};

use super::super::types::{ThreadSubject, ThreadSubjectKind};

const SIMPLE_RESPONSE_TIMEOUT: Duration = Duration::from_secs(90);
const STANDARD_RESPONSE_TIMEOUT: Duration = Duration::from_secs(240);
const DEEP_RESPONSE_TIMEOUT: Duration = Duration::from_secs(600);
const STANDARD_ACTION_PROJECTION_TIMEOUT: Duration = Duration::from_secs(120);
const DEEP_ACTION_PROJECTION_TIMEOUT: Duration = Duration::from_secs(300);

pub(super) struct AbortOnDropTask<T> {
    handle: JoinHandle<T>,
}

impl<T> AbortOnDropTask<T> {
    pub(super) fn new(handle: JoinHandle<T>) -> Self {
        Self { handle }
    }

    pub(super) async fn join(&mut self) -> Result<T, JoinError> {
        (&mut self.handle).await
    }
}

impl<T> Drop for AbortOnDropTask<T> {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

pub(super) fn response_timeout(complexity: TaskComplexity) -> Duration {
    match complexity {
        TaskComplexity::Simple => SIMPLE_RESPONSE_TIMEOUT,
        TaskComplexity::Standard => STANDARD_RESPONSE_TIMEOUT,
        TaskComplexity::Deep => DEEP_RESPONSE_TIMEOUT,
    }
}

pub(super) fn action_projection_timeout(task_complexity: TaskComplexity) -> Duration {
    match task_complexity {
        TaskComplexity::Deep => DEEP_ACTION_PROJECTION_TIMEOUT,
        TaskComplexity::Simple | TaskComplexity::Standard => STANDARD_ACTION_PROJECTION_TIMEOUT,
    }
}

pub(super) fn finish_visible_response(
    result: Result<Result<String, AiError>, Elapsed>,
    response_timeout: Duration,
) -> AppResult<String> {
    match result {
        Ok(Ok(response)) => Ok(response),
        Ok(Err(error)) => Err(AppError::internal(error.to_string())),
        Err(_) => Err(AppError::internal(format!(
            "AI response timed out after {} seconds",
            response_timeout.as_secs()
        ))),
    }
}

pub(super) fn enrich_draft(draft: &mut ConversationActionDraft, subject: &ThreadSubject) {
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

pub(super) fn fallback_summary(message: &str) -> String {
    let mut summary = message.trim().chars().take(240).collect::<String>();
    if message.chars().count() > 240 {
        summary.push_str("...");
    }
    summary
}

pub(super) fn should_skip_action_projection(
    message: &str,
    has_attachments: bool,
    has_research_sources: bool,
) -> bool {
    if has_attachments || has_research_sources {
        return false;
    }

    super::super::is_simple_social_turn(message)
}

pub(super) fn action_projection_complexity(subject: &ThreadSubject) -> TaskComplexity {
    if subject.kind_type() == ThreadSubjectKind::InvestmentSystem {
        TaskComplexity::Deep
    } else {
        TaskComplexity::Standard
    }
}

pub(super) fn action_type_allowed_for_subject(action_type: &str, subject: &ThreadSubject) -> bool {
    action_type != "rule_graph_patch" || subject.kind_type() == ThreadSubjectKind::InvestmentSystem
}

pub(super) fn casual_turn_summary(locale: Locale) -> &'static str {
    if locale.is_zh() {
        "用户进行了寒暄或能力询问，未产生可确认变更。"
    } else {
        "The user greeted the assistant or asked about its capabilities; no confirmable changes were proposed."
    }
}

pub(super) fn subject_clarification_summary(locale: Locale) -> &'static str {
    if locale.is_zh() {
        "等待用户确认公司全称或证券代码。"
    } else {
        "Waiting for the user to confirm the company name or security code."
    }
}
