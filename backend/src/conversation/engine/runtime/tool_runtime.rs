use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::{
    ai::ConversationResearchSource,
    conversation::{
        capabilities::{
            CapabilityExecutionContext, CapabilityKind, CompletedToolCall, FailedToolCall,
            ToolExecutionReport, ToolPlan, ToolStoragePolicy,
        },
        storage,
        types::PersistedResearchSource,
    },
    error::{AppError, AppResult},
};

use super::super::{
    events::ConversationEvent,
    task::TurnCancellation,
    turn_context::{StepContext, TurnContext},
    turn_support::AbortOnDropTask,
    ConversationEngine,
};

#[derive(Default)]
pub(super) struct TurnArtifacts {
    pub(super) model_sources: Vec<ConversationResearchSource>,
    pub(super) persisted_sources: Vec<PersistedResearchSource>,
    pub(super) artifacts: Vec<Value>,
    pub(super) warning: Option<String>,
    pub(super) had_failures: bool,
}

impl StepContext {
    pub(super) fn absorb(&mut self, mut other: TurnArtifacts) {
        self.model.research_sources.append(&mut other.model_sources);
        self.sources.append(&mut other.persisted_sources);
        self.model.research_warning =
            join_warnings(self.model.research_warning.take(), other.warning.take());
        for artifact in &other.artifacts {
            self.model.used_context.push(json!({
                "kind": "capability",
                "label": artifact.get("display_name").and_then(Value::as_str),
                "capability_id": artifact.get("capability_id").and_then(Value::as_str),
                "capability_kind": artifact.get("capability_kind").and_then(Value::as_str)
            }));
        }
        self.model
            .capability_artifacts
            .extend(other.artifacts.iter().cloned());
        self.artifacts.append(&mut other.artifacts);
    }
}

impl ConversationEngine {
    pub(super) async fn execute_tools(
        &self,
        turn: &TurnContext,
        tool_plan: ToolPlan,
        execution_context: CapabilityExecutionContext,
        cancellation: &TurnCancellation,
    ) -> AppResult<TurnArtifacts> {
        if !tool_plan.has_calls() {
            return Ok(TurnArtifacts::default());
        }
        cancellation.ensure_active()?;
        let (tool_event_tx, mut tool_event_rx) = mpsc::unbounded_channel();
        let tools = self.tools.clone();
        let tool_cancellation = cancellation.clone();
        let mut execution = AbortOnDropTask::new(tokio::spawn(async move {
            tools
                .execute(
                    tool_plan,
                    execution_context,
                    &tool_cancellation,
                    tool_event_tx,
                )
                .await
        }));
        let report = loop {
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => {
                    return Err(AppError::internal("conversation run canceled"));
                }
                event = tool_event_rx.recv() => {
                    let Some(event) = event else {
                        break execution.join().await.map_err(|error| {
                            AppError::internal(format!("tool orchestration task failed: {error}"))
                        })?;
                    };
                    self.handle_tool_lifecycle(&turn.run, event).await?;
                }
                result = execution.join() => {
                    break result.map_err(|error| {
                        AppError::internal(format!("tool orchestration task failed: {error}"))
                    })?;
                },
            }
        };
        while let Ok(event) = tool_event_rx.try_recv() {
            cancellation.ensure_active()?;
            self.handle_tool_lifecycle(&turn.run, event).await?;
        }
        cancellation.ensure_active()?;
        self.persist_tool_outputs(turn, report?, cancellation).await
    }

    async fn persist_tool_outputs(
        &self,
        turn: &TurnContext,
        report: ToolExecutionReport,
        cancellation: &TurnCancellation,
    ) -> AppResult<TurnArtifacts> {
        let mut artifacts = TurnArtifacts {
            had_failures: !report.failures.is_empty(),
            ..TurnArtifacts::default()
        };
        let mut warnings = Vec::new();
        for FailedToolCall {
            call_id,
            tool_name,
            tool_version,
            capability_kind,
            display_name,
            subject_label,
            manifest_hash,
            duration_ms,
            storage_policy,
            code,
            message,
        } in report.failures
        {
            tracing::warn!(%call_id, %tool_name, %code, %message, "conversation tool failed");
            if storage_policy == Some(ToolStoragePolicy::StructuredArtifact) {
                artifacts.artifacts.push(json!({
                    "call_id": call_id,
                    "capability_id": tool_name,
                    "capability_version": tool_version,
                    "capability_kind": capability_kind.map(CapabilityKind::as_str),
                    "display_name": display_name,
                    "artifact_type": "capability_failure",
                    "subject_label": subject_label,
                    "status": "failed",
                    "payload": {},
                    "source_ids": [],
                    "error_code": code,
                    "error_message": message,
                    "manifest_hash": manifest_hash,
                    "duration_ms": duration_ms,
                    "execution_steps": 0,
                    "model_steps": [],
                    "agent_trace": []
                }));
            } else {
                warnings.push(message);
            }
        }
        for CompletedToolCall {
            call_id,
            tool_name,
            tool_version,
            capability_kind,
            display_name,
            subject_label,
            manifest_hash,
            duration_ms,
            storage_policy,
            output,
        } in report.completed
        {
            tracing::debug!(%call_id, %tool_name, "conversation tool completed");
            let warning = output.warning.clone();
            if storage_policy == ToolStoragePolicy::SourcesAndSummary {
                if let Some(warning) = &warning {
                    warnings.push(warning.clone());
                }
            }
            let mut source_ids = Vec::new();
            for source in output.sources {
                cancellation.ensure_active()?;
                let (persisted, inserted) =
                    storage::insert_source(&self.pool, &turn.run.id, &source).await?;
                if inserted {
                    self.emit(
                        &turn.run.id,
                        &turn.run.thread_id,
                        ConversationEvent::SourceAdded(persisted.clone()),
                    )
                    .await?;
                    artifacts.persisted_sources.push(persisted.clone());
                    artifacts.model_sources.push(source);
                }
                source_ids.push(persisted.id.clone());
            }
            if storage_policy == ToolStoragePolicy::StructuredArtifact {
                artifacts.artifacts.push(json!({
                    "call_id": call_id,
                    "capability_id": tool_name,
                    "capability_version": tool_version,
                    "capability_kind": capability_kind.as_str(),
                    "display_name": display_name,
                    "artifact_type": output.artifact_type,
                    "subject_label": subject_label,
                    "status": "completed",
                    "payload": output.payload,
                    "source_ids": source_ids,
                    "warning": warning,
                    "provider": output.model_route.as_ref().map(|route| route.provider.as_str()),
                    "model": output.model_route.as_ref().map(|route| route.model.as_str()),
                    "model_steps": output.model_routes,
                    "manifest_hash": manifest_hash,
                    "duration_ms": duration_ms,
                    "execution_steps": output.execution_steps,
                    "agent_trace": output.agent_trace
                }));
            }
        }
        artifacts.warning = (!warnings.is_empty()).then(|| warnings.join(" "));
        Ok(artifacts)
    }
}

fn join_warnings(left: Option<String>, right: Option<String>) -> Option<String> {
    match (left, right) {
        (Some(left), Some(right)) => Some(format!("{left} {right}")),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}
