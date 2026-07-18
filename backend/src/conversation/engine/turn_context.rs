use serde_json::Value;

use crate::{
    ai::{runtime::TaskComplexity, ConversationContext, ConversationSubjectClarification},
    error::AppResult,
    locale::Locale,
};

use super::super::{
    capabilities::ToolPlan,
    task_routing::TaskRouteReason,
    types::{ConversationExecutionPlan, ConversationRun, PersistedResearchSource, ThreadSubject},
};

pub(super) struct TurnContext {
    pub(super) run: ConversationRun,
    pub(super) locale: Locale,
    pub(super) user_message: String,
    pub(super) effective_user_message: String,
    pub(super) subject: ThreadSubject,
    pub(super) clarification: Option<ConversationSubjectClarification>,
    pub(super) task_complexity: TaskComplexity,
    pub(super) route_reason: TaskRouteReason,
    pub(super) tool_plan: ToolPlan,
    pub(super) execution_plan: Option<ConversationExecutionPlan>,
}

pub(super) struct StepContext {
    pub(super) turn: TurnContext,
    pub(super) model: ConversationContext,
    pub(super) sources: Vec<PersistedResearchSource>,
    pub(super) artifacts: Vec<Value>,
}

impl StepContext {
    pub(super) fn source_payloads(&self) -> AppResult<Vec<Value>> {
        self.sources
            .iter()
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }
}
