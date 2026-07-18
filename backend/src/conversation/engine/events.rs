use serde_json::{json, Value};

use crate::error::AppResult;

use super::super::{
    capabilities::ToolLifecycleEvent,
    types::{
        ConversationAction, ConversationExecutionPlan, ConversationRun, ConversationThreadSummary,
        PersistedResearchSource,
    },
};

pub(super) enum ConversationEvent {
    RunAccepted {
        run: ConversationRun,
        thread: ConversationThreadSummary,
    },
    RunRetried {
        run: ConversationRun,
        thread: ConversationThreadSummary,
        retry_of_run_id: String,
    },
    RunInterrupted {
        code: String,
        retryable: bool,
    },
    RunCanceled {
        retryable: bool,
    },
    RunClassified {
        run: ConversationRun,
        task_complexity: String,
        route_reason: String,
    },
    RunRouted {
        run: ConversationRun,
        provider: String,
        model: String,
        task_complexity: String,
        route_reason: String,
    },
    RunPhase {
        phase: String,
        provider: Option<String>,
        detail: Option<Value>,
    },
    RunPlanCreated {
        plan: ConversationExecutionPlan,
    },
    RunPlanStep {
        step_id: String,
        status: String,
    },
    ProviderPhase {
        provider: String,
        provider_stage: String,
    },
    MessageDelta {
        message_id: String,
        content: String,
    },
    MessageCompleted {
        message_id: String,
        content: Option<String>,
    },
    Tool(ToolLifecycleEvent),
    SourceAdded(PersistedResearchSource),
    ActionProposed(ConversationAction),
    ActionUpdated(ConversationAction),
    RunWarning {
        code: String,
        message: String,
    },
    RunCompleted {
        message_id: String,
    },
    RunFailed {
        code: String,
        message: String,
        retryable: bool,
    },
}

impl ConversationEvent {
    pub(super) fn into_wire(self) -> AppResult<(&'static str, Value)> {
        let wire = match self {
            Self::RunAccepted { run, thread } => {
                ("run.accepted", json!({ "run": run, "thread": thread }))
            }
            Self::RunRetried {
                run,
                thread,
                retry_of_run_id,
            } => (
                "run.accepted",
                json!({
                    "run": run,
                    "thread": thread,
                    "retry_of_run_id": retry_of_run_id
                }),
            ),
            Self::RunInterrupted { code, retryable } => (
                "run.interrupted",
                json!({ "code": code, "retryable": retryable }),
            ),
            Self::RunCanceled { retryable } => ("run.canceled", json!({ "retryable": retryable })),
            Self::RunClassified {
                run,
                task_complexity,
                route_reason,
            } => (
                "run.classified",
                json!({
                    "run": run,
                    "task_complexity": task_complexity,
                    "route_reason": route_reason
                }),
            ),
            Self::RunRouted {
                run,
                provider,
                model,
                task_complexity,
                route_reason,
            } => (
                "run.routed",
                json!({
                    "run": run,
                    "provider": provider,
                    "model": model,
                    "task_complexity": task_complexity,
                    "route_reason": route_reason
                }),
            ),
            Self::RunPhase {
                phase,
                provider,
                detail,
            } => (
                "run.phase",
                json!({ "phase": phase, "provider": provider, "detail": detail }),
            ),
            Self::RunPlanCreated { plan } => ("run.plan.created", serde_json::to_value(plan)?),
            Self::RunPlanStep { step_id, status } => (
                "run.plan.step",
                json!({ "step_id": step_id, "status": status }),
            ),
            Self::ProviderPhase {
                provider,
                provider_stage,
            } => (
                "run.phase",
                json!({
                    "phase": "generating",
                    "provider": provider,
                    "provider_stage": provider_stage
                }),
            ),
            Self::MessageDelta {
                message_id,
                content,
            } => (
                "message.delta",
                json!({ "message_id": message_id, "content": content }),
            ),
            Self::MessageCompleted {
                message_id,
                content,
            } => {
                let mut payload = json!({ "message_id": message_id });
                if let Some(content) = content {
                    payload["content"] = Value::String(content);
                }
                ("message.completed", payload)
            }
            Self::Tool(event) => tool_event_wire(event),
            Self::SourceAdded(source) => ("source.added", serde_json::to_value(source)?),
            Self::ActionProposed(action) => ("action.proposed", serde_json::to_value(action)?),
            Self::ActionUpdated(action) => ("action.updated", serde_json::to_value(action)?),
            Self::RunWarning { code, message } => {
                ("run.warning", json!({ "code": code, "message": message }))
            }
            Self::RunCompleted { message_id } => {
                ("run.completed", json!({ "message_id": message_id }))
            }
            Self::RunFailed {
                code,
                message,
                retryable,
            } => (
                "run.failed",
                json!({ "code": code, "message": message, "retryable": retryable }),
            ),
        };
        Ok(wire)
    }
}

fn tool_event_wire(event: ToolLifecycleEvent) -> (&'static str, Value) {
    match event {
        ToolLifecycleEvent::Started {
            call_id,
            tool_name,
            tool_version,
            capability_kind,
            display_name,
            stage,
            step_index,
            total_steps,
            activity,
            subject_label,
            cache_policy,
            storage_policy,
        } => (
            "tool.started",
            json!({
                "call_id": call_id,
                "tool_name": tool_name,
                "tool_version": tool_version,
                "capability_kind": capability_kind,
                "display_name": display_name,
                "stage": stage,
                "step_index": step_index,
                "total_steps": total_steps,
                "activity": activity,
                "subject_label": subject_label,
                "cache_policy": cache_policy,
                "storage_policy": storage_policy
            }),
        ),
        ToolLifecycleEvent::Progress {
            call_id,
            tool_name,
            tool_version,
            capability_kind,
            display_name,
            stage,
            step_index,
            total_steps,
            activity,
            detail,
            subject_label,
        } => {
            let nested_tool_name = detail
                .as_ref()
                .and_then(|detail| detail.nested_tool_name.as_deref());
            let nested_tool_display_name = detail
                .as_ref()
                .and_then(|detail| detail.nested_tool_display_name.as_deref());
            let agent_turn = detail.as_ref().and_then(|detail| detail.agent_turn);
            let agent_turn_limit = detail.as_ref().and_then(|detail| detail.agent_turn_limit);
            (
                "tool.progress",
                json!({
                "call_id": call_id,
                "tool_name": tool_name,
                "tool_version": tool_version,
                "capability_kind": capability_kind,
                "display_name": display_name,
                "stage": stage,
                "step_index": step_index,
                "total_steps": total_steps,
                "activity": activity,
                "subject_label": subject_label,
                "nested_tool_name": nested_tool_name,
                "nested_tool_display_name": nested_tool_display_name,
                "agent_turn": agent_turn,
                "agent_turn_limit": agent_turn_limit
                }),
            )
        }
        ToolLifecycleEvent::Completed {
            call_id,
            tool_name,
            tool_version,
            capability_kind,
            display_name,
            stage,
            step_index,
            total_steps,
            duration_ms,
            source_count,
            warning,
        } => (
            "tool.completed",
            json!({
                "call_id": call_id,
                "tool_name": tool_name,
                "tool_version": tool_version,
                "capability_kind": capability_kind,
                "display_name": display_name,
                "stage": stage,
                "step_index": step_index,
                "total_steps": total_steps,
                "duration_ms": duration_ms,
                "source_count": source_count,
                "warning": warning
            }),
        ),
        ToolLifecycleEvent::Failed {
            call_id,
            tool_name,
            tool_version,
            capability_kind,
            display_name,
            stage,
            step_index,
            total_steps,
            duration_ms,
            code,
            message,
        } => (
            "tool.failed",
            json!({
                "call_id": call_id,
                "tool_name": tool_name,
                "tool_version": tool_version,
                "capability_kind": capability_kind,
                "display_name": display_name,
                "stage": stage,
                "step_index": step_index,
                "total_steps": total_steps,
                "duration_ms": duration_ms,
                "code": code,
                "message": message
            }),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::types::ConversationExecutionPlanStep;

    #[test]
    fn phase_events_preserve_the_existing_frontend_contract() {
        let (event_type, payload) = ConversationEvent::RunPhase {
            phase: "researching".to_string(),
            provider: None,
            detail: Some(json!({ "activity": "research_searching_official" })),
        }
        .into_wire()
        .expect("serialize event");

        assert_eq!(event_type, "run.phase");
        assert_eq!(
            payload,
            json!({
                "phase": "researching",
                "provider": null,
                "detail": { "activity": "research_searching_official" }
            })
        );

        let (_, provider_payload) = ConversationEvent::ProviderPhase {
            provider: "cli".to_string(),
            provider_stage: "model_running".to_string(),
        }
        .into_wire()
        .expect("serialize event");
        assert_eq!(
            provider_payload,
            json!({
                "phase": "generating",
                "provider": "cli",
                "provider_stage": "model_running"
            })
        );
    }

    #[test]
    fn completed_message_omits_content_after_real_streaming() {
        let (_, payload) = ConversationEvent::MessageCompleted {
            message_id: "message-1".to_string(),
            content: None,
        }
        .into_wire()
        .expect("serialize event");

        assert_eq!(payload, json!({ "message_id": "message-1" }));
    }

    #[test]
    fn run_plan_events_expose_scope_and_step_status_without_prompt_text() {
        let (event_type, payload) = ConversationEvent::RunPlanCreated {
            plan: ConversationExecutionPlan {
                template_id: "company_analysis_v1".to_string(),
                scope: "moat".to_string(),
                dimensions: vec!["business_model".to_string(), "moat".to_string()],
                steps: vec![ConversationExecutionPlanStep {
                    id: "research".to_string(),
                    status: "pending".to_string(),
                }],
            },
        }
        .into_wire()
        .expect("serialize plan");
        let (step_type, step_payload) = ConversationEvent::RunPlanStep {
            step_id: "research".to_string(),
            status: "running".to_string(),
        }
        .into_wire()
        .expect("serialize plan step");

        assert_eq!(event_type, "run.plan.created");
        assert_eq!(payload["scope"], "moat");
        assert!(payload.get("prompt").is_none());
        assert_eq!(step_type, "run.plan.step");
        assert_eq!(step_payload["step_id"], "research");
    }

    #[test]
    fn source_events_are_not_wrapped_in_an_internal_envelope() {
        let source = PersistedResearchSource {
            id: "source-1".to_string(),
            title: "Annual report".to_string(),
            url: "https://example.com/report".to_string(),
            snippet: "Evidence".to_string(),
            source_tier: "primary".to_string(),
            retrieved_at: "2026-07-15T00:00:00Z".to_string(),
        };
        let (event_type, payload) = ConversationEvent::SourceAdded(source)
            .into_wire()
            .expect("serialize event");

        assert_eq!(event_type, "source.added");
        assert_eq!(payload["id"], "source-1");
        assert!(payload.get("source").is_none());
    }

    #[test]
    fn tool_events_persist_metadata_without_raw_arguments_or_results() {
        let (event_type, payload) = ConversationEvent::Tool(ToolLifecycleEvent::Started {
            call_id: "run-1:tool:1".to_string(),
            tool_name: "research_company".to_string(),
            tool_version: 1,
            capability_kind: super::super::super::capabilities::CapabilityKind::Native,
            display_name: "Company research".to_string(),
            stage: super::super::super::capabilities::CapabilityStage::Research,
            step_index: 1,
            total_steps: 1,
            activity: "research_preparing_company".to_string(),
            subject_label: Some("Tencent".to_string()),
            cache_policy: super::super::super::capabilities::ToolCachePolicy::DailyProviderCache,
            storage_policy: super::super::super::capabilities::ToolStoragePolicy::SourcesAndSummary,
        })
        .into_wire()
        .expect("serialize event");

        assert_eq!(event_type, "tool.started");
        assert_eq!(payload["tool_name"], "research_company");
        assert_eq!(payload["tool_version"], 1);
        assert_eq!(payload["capability_kind"], "native");
        assert_eq!(payload["stage"], "research");
        assert_eq!(payload["subject_label"], "Tencent");
        assert!(payload.get("arguments").is_none());
        assert!(payload.get("result").is_none());
    }

    #[test]
    fn agent_progress_persists_nested_read_tool_without_arguments_or_results() {
        use super::super::super::capabilities::{
            CapabilityKind, CapabilityStage, ToolProgressDetail,
        };

        let (event_type, payload) = ConversationEvent::Tool(ToolLifecycleEvent::Progress {
            call_id: "run-1:tool:2".to_string(),
            tool_name: "analyze_company".to_string(),
            tool_version: 1,
            capability_kind: CapabilityKind::Agent,
            display_name: "Company analysis".to_string(),
            stage: CapabilityStage::Analysis,
            step_index: 2,
            total_steps: 2,
            activity: "agent_calling_read_only_tool".to_string(),
            detail: Some(ToolProgressDetail {
                nested_tool_name: Some("research_company".to_string()),
                nested_tool_display_name: Some("Company research".to_string()),
                agent_turn: Some(2),
                agent_turn_limit: Some(8),
            }),
            subject_label: Some("Tencent".to_string()),
        })
        .into_wire()
        .expect("serialize progress event");

        assert_eq!(event_type, "tool.progress");
        assert_eq!(payload["nested_tool_name"], "research_company");
        assert_eq!(payload["agent_turn"], 2);
        assert!(payload.get("arguments").is_none());
        assert!(payload.get("result").is_none());
    }
}
