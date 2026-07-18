use serde::Deserialize;
use serde_json::Value;

use super::super::{schema::validate_json_schema, ToolExecutionError};

#[derive(Deserialize)]
pub(super) struct AgentDecision {
    pub(super) action: AgentAction,
    pub(super) tool_id: String,
    pub(super) tool_version: u16,
    pub(super) arguments: Value,
    pub(super) output: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum AgentAction {
    Tool,
    Final,
}

pub(super) fn validate_final_decision(
    decision: &AgentDecision,
    output_schema: &Value,
) -> Result<(), ToolExecutionError> {
    if !decision.tool_id.is_empty()
        || decision.tool_version != 0
        || decision
            .arguments
            .as_object()
            .is_none_or(|value| !value.is_empty())
    {
        return Err(ToolExecutionError::new(
            "invalid_agent_decision",
            "final agent decisions cannot contain a tool call",
        ));
    }
    validate_json_schema(&decision.output, output_schema, "agent final output")
}

pub(super) fn validate_tool_decision(decision: &AgentDecision) -> Result<(), ToolExecutionError> {
    if decision.tool_id.is_empty()
        || decision.tool_version == 0
        || !decision.arguments.is_object()
        || decision
            .output
            .as_object()
            .is_none_or(|value| !value.is_empty())
    {
        return Err(ToolExecutionError::new(
            "invalid_agent_decision",
            "tool decisions require one tool and cannot contain final output",
        ));
    }
    Ok(())
}
