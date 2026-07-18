use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use serde_json::{json, Value};
use sqlx::SqlitePool;
use tokio::sync::mpsc;

use super::{
    CapabilityExecutionContext, ConversationTool, NativeToolDescriptor, ResearchToolInput,
    ToolCachePolicy, ToolDescriptor, ToolExecutionError, ToolOutput, ToolProgress,
    ToolStoragePolicy, RESEARCH_COMPANY_TOOL,
};
use crate::conversation::research::{execute_with_cache, ResearchProgress, WebResearchProvider};
use crate::conversation::{research::plan_research, types::ThreadSubject};

const RESEARCH_TOOL_TIMEOUT: Duration = Duration::from_secs(600);

pub(super) struct ResearchCompanyTool {
    pool: SqlitePool,
    provider: Arc<dyn WebResearchProvider>,
}

impl ResearchCompanyTool {
    pub(super) fn new(pool: SqlitePool, provider: Arc<dyn WebResearchProvider>) -> Self {
        Self { pool, provider }
    }
}

#[async_trait]
impl ConversationTool for ResearchCompanyTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor::native(
            NativeToolDescriptor {
                name: RESEARCH_COMPANY_TOOL,
                display_name: "Company research",
                description: "Retrieve and URL-validate bounded company operating evidence for one material gap",
                timeout: RESEARCH_TOOL_TIMEOUT,
                initial_activity: "research_preparing_company",
                cache_policy: ToolCachePolicy::DailyProviderCache,
                storage_policy: ToolStoragePolicy::SourcesAndSummary,
            },
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["plan"],
                "properties": {
                    "plan": {
                        "type": "object",
                        "description": "A deterministic company research plan"
                    }
                }
            }),
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["source_count", "has_warning"],
                "properties": {
                    "source_count": { "type": "integer" },
                    "has_warning": { "type": "boolean" }
                }
            }),
        )
    }

    fn agent_input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["focus"],
            "properties": {
                "focus": {
                    "type": "string",
                    "maxLength": 4000,
                    "description": "One decision-changing factual gap to research; not a raw query or broad company-analysis request"
                }
            }
        }))
    }

    fn prepare_agent_arguments(
        &self,
        arguments: Value,
        context: &CapabilityExecutionContext,
    ) -> Result<Value, ToolExecutionError> {
        let focus = arguments
            .get("focus")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ToolExecutionError::new("invalid_tool_arguments", "focus is required")
            })?;
        let conversation = context.conversation.as_deref().ok_or_else(|| {
            ToolExecutionError::new(
                "capability_context_unavailable",
                "company research requires conversation context",
            )
        })?;
        let subject = serde_json::from_value::<ThreadSubject>(conversation.subject.clone())
            .map_err(|error| {
                ToolExecutionError::new("invalid_subject", format!("invalid subject: {error}"))
            })?;
        let plan = plan_research(focus, &subject).ok_or_else(|| {
            ToolExecutionError::new(
                "research_not_applicable",
                "company research is unavailable for this subject",
            )
        })?;
        serde_json::to_value(ResearchToolInput { plan })
            .map_err(|error| ToolExecutionError::new("invalid_tool_arguments", error.to_string()))
    }

    async fn invoke(
        &self,
        arguments: Value,
        _context: CapabilityExecutionContext,
        progress: mpsc::UnboundedSender<ToolProgress>,
    ) -> Result<ToolOutput, ToolExecutionError> {
        let input: ResearchToolInput = serde_json::from_value(arguments).map_err(|error| {
            ToolExecutionError::new(
                "invalid_tool_arguments",
                format!("invalid research_company arguments: {error}"),
            )
        })?;
        let (research_tx, mut research_rx) = mpsc::unbounded_channel();
        let execution =
            execute_with_cache(&self.pool, self.provider.clone(), &input.plan, research_tx);
        tokio::pin!(execution);
        let mut progress_open = true;
        let outcome = loop {
            tokio::select! {
                progress_event = research_rx.recv(), if progress_open => {
                    match progress_event {
                        Some(progress_event) => send_progress(&progress, progress_event)?,
                        None => progress_open = false,
                    }
                }
                result = &mut execution => break result,
            }
        };
        while let Ok(progress_event) = research_rx.try_recv() {
            send_progress(&progress, progress_event)?;
        }
        outcome
            .map(|outcome| ToolOutput::research("company_research", outcome))
            .map_err(|error| ToolExecutionError::new("research_failed", error.to_string()))
    }
}

fn send_progress(
    progress: &mpsc::UnboundedSender<ToolProgress>,
    event: ResearchProgress,
) -> Result<(), ToolExecutionError> {
    progress
        .send(ToolProgress::activity(event.code()))
        .map_err(|_| {
            ToolExecutionError::new("tool_progress_closed", "tool progress receiver closed")
        })
}
