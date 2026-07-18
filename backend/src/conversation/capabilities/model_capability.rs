use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use serde_json::{Map, Value};
use tokio::sync::mpsc;

use crate::ai::{runtime::AiRuntime, CapabilityModelRequest, ConversationContext};

use super::{
    manifest::CapabilityDefinition,
    schema::{context_source_urls, validate_evidence_urls, validate_json_schema},
    CapabilityContextKey, CapabilityExecutionContext, CapabilityModelRoute, ConversationTool,
    ToolCachePolicy, ToolConfirmation, ToolDescriptor, ToolExecutionError, ToolOutput,
    ToolProgress, ToolSideEffect, ToolStoragePolicy,
};

pub(super) struct SkillCapabilityTool {
    definition: CapabilityDefinition,
    ai: Arc<AiRuntime>,
}

impl SkillCapabilityTool {
    pub(super) fn new(definition: CapabilityDefinition, ai: Arc<AiRuntime>) -> Self {
        debug_assert_eq!(definition.manifest.kind, super::CapabilityKind::Skill);
        Self { definition, ai }
    }
}

#[async_trait]
impl ConversationTool for SkillCapabilityTool {
    fn descriptor(&self) -> ToolDescriptor {
        let manifest = &self.definition.manifest;
        ToolDescriptor {
            name: manifest.id.clone(),
            version: manifest.version,
            kind: manifest.kind,
            stage: manifest.stage,
            display_name: manifest.display_name.clone(),
            description: manifest.description.clone(),
            artifact_type: manifest.artifact_type.clone(),
            input_schema: manifest.input_schema.clone(),
            output_schema: manifest.output_schema.clone(),
            context: manifest.context.clone(),
            model: Some(manifest.model),
            max_steps: manifest.max_steps,
            tools: manifest.tools.clone(),
            skills: manifest.skills.clone(),
            surfaces: manifest.surfaces.clone(),
            subjects: manifest.subjects.clone(),
            triggers: manifest.triggers.clone(),
            manifest_hash: self.definition.content_hash.clone(),
            side_effect: ToolSideEffect::ReadOnly,
            confirmation: ToolConfirmation::Automatic,
            cache_policy: ToolCachePolicy::None,
            storage_policy: ToolStoragePolicy::StructuredArtifact,
            timeout: Duration::from_secs(manifest.timeout_seconds),
            initial_activity: manifest.initial_activity.clone(),
        }
    }

    async fn invoke(
        &self,
        arguments: Value,
        context: CapabilityExecutionContext,
        progress: mpsc::UnboundedSender<ToolProgress>,
    ) -> Result<ToolOutput, ToolExecutionError> {
        let manifest = &self.definition.manifest;
        let context_snapshot = capability_context(&context, &manifest.context)?;
        let allowed_evidence_urls = context_source_urls(&context_snapshot);
        let _ = progress.send(ToolProgress::activity("skill_applying_method"));
        let request = CapabilityModelRequest {
            capability_id: manifest.id.clone(),
            capability_kind: "skill".to_string(),
            instructions: manifest.instructions.clone(),
            arguments,
            context: context_snapshot,
            output_schema: manifest.output_schema.clone(),
            step: 1,
            max_steps: 1,
            previous_output: None,
        };
        let execution = self
            .ai
            .execute_capability(&request, context.locale, manifest.model)
            .await
            .map_err(|error| {
                ToolExecutionError::new("capability_model_failed", error.to_string())
            })?;
        validate_json_schema(
            &execution.output,
            &manifest.output_schema,
            "capability model output",
        )?;
        validate_evidence_urls(&execution.output, &allowed_evidence_urls)?;
        let route = CapabilityModelRoute {
            step: 1,
            provider: execution.provider,
            model: execution.model,
        };
        Ok(ToolOutput {
            artifact_type: manifest.artifact_type.clone(),
            payload: execution.output,
            sources: Vec::new(),
            warning: None,
            model_route: Some(route.clone()),
            model_routes: vec![route],
            execution_steps: 1,
            agent_trace: Vec::new(),
        })
    }
}

pub(super) fn capability_context(
    execution: &CapabilityExecutionContext,
    allowed: &[CapabilityContextKey],
) -> Result<Value, ToolExecutionError> {
    if let Some(context) = execution.conversation.as_deref() {
        return Ok(snapshot(context, allowed));
    }
    if !allowed.contains(&CapabilityContextKey::RuleGraphInput) {
        return Err(ToolExecutionError::new(
            "capability_context_forbidden",
            "capability does not grant rule_graph_input context access",
        ));
    }
    execution.rule_graph.clone().ok_or_else(|| {
        ToolExecutionError::new(
            "capability_context_unavailable",
            "model capability requires a frozen conversation or rule graph context",
        )
    })
}

fn snapshot(context: &ConversationContext, allowed: &[CapabilityContextKey]) -> Value {
    let mut output = Map::new();
    for key in allowed {
        match key {
            CapabilityContextKey::Subject => {
                output.insert("subject".to_string(), context.subject.clone());
            }
            CapabilityContextKey::UserMessage => {
                output.insert(
                    "user_message".to_string(),
                    Value::String(context.user_message.clone()),
                );
            }
            CapabilityContextKey::CompanyView => {
                output.insert(
                    "company_view".to_string(),
                    context.company_view.clone().unwrap_or(Value::Null),
                );
            }
            CapabilityContextKey::ResearchSources => {
                output.insert(
                    "research_sources".to_string(),
                    serde_json::to_value(&context.research_sources).unwrap_or(Value::Array(vec![])),
                );
            }
            CapabilityContextKey::Attachments => {
                output.insert(
                    "attachments".to_string(),
                    serde_json::to_value(&context.attachments).unwrap_or(Value::Array(vec![])),
                );
            }
            CapabilityContextKey::ConversationHistory => {
                output.insert(
                    "conversation_history".to_string(),
                    serde_json::json!({
                        "thread_summary": context.thread_summary,
                        "turn_summaries": context.turn_summaries,
                        "recent_messages": context.recent_messages,
                    }),
                );
            }
            CapabilityContextKey::Portfolio => {
                output.insert(
                    "portfolio".to_string(),
                    serde_json::json!({
                        "summary": context.portfolio_summary,
                        "positions": context.portfolio_positions,
                    }),
                );
            }
            CapabilityContextKey::InvestmentSystem => {
                output.insert(
                    "investment_system".to_string(),
                    context.investment_system.clone(),
                );
            }
            CapabilityContextKey::RuleGraphInput => {}
        }
    }
    Value::Object(output)
}
