use serde_json::{json, Value};

use crate::error::AppResult;

use super::super::types::{
    ConversationAction, ConversationRun, ConversationThreadSummary, PersistedResearchSource,
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
