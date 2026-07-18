use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::{
    ai::runtime::AiRuntime,
    ai::{prompt::agent_decision_schema, AgentModelRequest, AgentModelSkill, AgentModelTool},
};

use super::{
    manifest::{CapabilityDefinition, CapabilityReference},
    model_capability::capability_context,
    registry::ToolRegistry,
    schema::{context_source_urls, validate_evidence_urls, validate_json_schema},
    CapabilityExecutionContext, CapabilityKind, CapabilityModelRoute, ConversationTool,
    ToolCachePolicy, ToolConfirmation, ToolDescriptor, ToolExecutionError, ToolOutput,
    ToolProgress, ToolSideEffect, ToolStoragePolicy,
};

mod decision;
mod evidence;

use decision::{validate_final_decision, validate_tool_decision, AgentAction, AgentDecision};
use evidence::AgentEvidence;

const MAX_AGENT_TOOL_CALLS: usize = 4;
const MAX_LOADED_SKILL_INSTRUCTIONS_CHARS: usize = 48_000;

pub(super) struct AgentCapabilityTool {
    definition: CapabilityDefinition,
    ai: Arc<AiRuntime>,
    toolbox: Arc<ToolRegistry>,
    available_tools: Vec<AgentModelTool>,
    loaded_skills: Vec<AgentModelSkill>,
}

impl AgentCapabilityTool {
    pub(super) fn new(
        definition: CapabilityDefinition,
        ai: Arc<AiRuntime>,
        toolbox: Arc<ToolRegistry>,
        skill_catalog: &HashMap<CapabilityReference, CapabilityDefinition>,
    ) -> Result<Self, ToolExecutionError> {
        if definition.manifest.kind != CapabilityKind::Agent {
            return Err(ToolExecutionError::new(
                "invalid_agent_definition",
                "AgentCapabilityTool requires an agent manifest",
            ));
        }
        let available_tools = definition
            .manifest
            .tools
            .iter()
            .map(|reference| {
                toolbox.agent_tool_spec(
                    reference,
                    &definition.manifest.surfaces,
                    &definition.manifest.subjects,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let loaded_skills = definition
            .manifest
            .skills
            .iter()
            .map(|reference| {
                let skill = skill_catalog.get(reference).ok_or_else(|| {
                    ToolExecutionError::new(
                        "capability_dependency_missing",
                        format!(
                            "agent references unavailable skill '{}@{}'",
                            reference.id, reference.version
                        ),
                    )
                })?;
                if skill.manifest.kind != CapabilityKind::Skill {
                    return Err(ToolExecutionError::new(
                        "invalid_agent_skill",
                        format!("'{}@{}' is not a skill", reference.id, reference.version),
                    ));
                }
                if !definition
                    .manifest
                    .surfaces
                    .iter()
                    .all(|surface| skill.manifest.surfaces.contains(surface))
                    || !definition
                        .manifest
                        .subjects
                        .iter()
                        .all(|subject| skill.manifest.subjects.contains(subject))
                {
                    return Err(ToolExecutionError::new(
                        "capability_dependency_scope_mismatch",
                        format!(
                            "skill '{}@{}' does not cover every agent surface and subject",
                            reference.id, reference.version
                        ),
                    ));
                }
                Ok(AgentModelSkill {
                    id: skill.manifest.id.clone(),
                    version: skill.manifest.version,
                    description: skill.manifest.description.clone(),
                    instructions: skill.manifest.instructions.clone(),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let loaded_skill_chars = loaded_skills
            .iter()
            .map(|skill| skill.instructions.chars().count())
            .sum::<usize>();
        if loaded_skill_chars > MAX_LOADED_SKILL_INSTRUCTIONS_CHARS {
            return Err(ToolExecutionError::new(
                "agent_skill_budget_exceeded",
                format!(
                    "loaded skill instructions exceed {MAX_LOADED_SKILL_INSTRUCTIONS_CHARS} characters"
                ),
            ));
        }
        Ok(Self {
            definition,
            ai,
            toolbox,
            available_tools,
            loaded_skills,
        })
    }

    async fn execute_nested_tool(
        &self,
        reference: &CapabilityReference,
        arguments: Value,
        context: CapabilityExecutionContext,
        progress: &mpsc::UnboundedSender<ToolProgress>,
        turn: u8,
        display_name: &str,
    ) -> Result<ToolOutput, ToolExecutionError> {
        let (nested_tx, mut nested_rx) = mpsc::unbounded_channel();
        let execution = self
            .toolbox
            .execute_agent_tool(reference, arguments, context, nested_tx);
        tokio::pin!(execution);
        let mut progress_open = true;
        let output = loop {
            tokio::select! {
                event = nested_rx.recv(), if progress_open => {
                    match event {
                        Some(event) => {
                            let _ = progress.send(ToolProgress::agent(
                                event.activity,
                                turn,
                                self.definition.manifest.max_steps,
                                Some((&reference.id, display_name)),
                            ));
                        }
                        None => progress_open = false,
                    }
                }
                result = &mut execution => break result,
            }
        };
        while let Ok(event) = nested_rx.try_recv() {
            let _ = progress.send(ToolProgress::agent(
                event.activity,
                turn,
                self.definition.manifest.max_steps,
                Some((&reference.id, display_name)),
            ));
        }
        output
    }
}

#[async_trait]
impl ConversationTool for AgentCapabilityTool {
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
            timeout: std::time::Duration::from_secs(manifest.timeout_seconds),
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
        let frozen_context = capability_context(&context, &manifest.context)?;
        let mut allowed_evidence_urls = context_source_urls(&frozen_context);
        let mut routes = Vec::new();
        let mut evidence = AgentEvidence::default();
        let mut seen_calls = HashSet::new();

        for turn in 1..=manifest.max_steps {
            let _ = progress.send(ToolProgress::agent(
                "agent_planning_next_step",
                turn,
                manifest.max_steps,
                None,
            ));
            let request = AgentModelRequest {
                agent_id: manifest.id.clone(),
                instructions: manifest.instructions.clone(),
                arguments: arguments.clone(),
                context: frozen_context.clone(),
                final_output_schema: manifest.output_schema.clone(),
                available_tools: self.available_tools.clone(),
                loaded_skills: self.loaded_skills.clone(),
                observations: evidence.observations(),
                turn,
                max_turns: manifest.max_steps,
            };
            let execution = self
                .ai
                .execute_agent_turn(&request, context.locale, manifest.model)
                .await
                .map_err(|error| {
                    ToolExecutionError::new("capability_model_failed", error.to_string())
                })?;
            validate_json_schema(
                &execution.output,
                &agent_decision_schema(&request),
                "agent decision",
            )?;
            let decision =
                serde_json::from_value::<AgentDecision>(execution.output).map_err(|error| {
                    ToolExecutionError::new("invalid_agent_decision", error.to_string())
                })?;
            routes.push(CapabilityModelRoute {
                step: turn,
                provider: execution.provider,
                model: execution.model,
            });

            match decision.action {
                AgentAction::Final => {
                    validate_final_decision(&decision, &manifest.output_schema)?;
                    allowed_evidence_urls.extend(evidence.source_urls().iter().cloned());
                    validate_evidence_urls(&decision.output, &allowed_evidence_urls)?;
                    let _ = progress.send(ToolProgress::agent(
                        "agent_synthesizing_result",
                        turn,
                        manifest.max_steps,
                        None,
                    ));
                    evidence.record_final(turn);
                    let (sources, warning, trace) = evidence.into_parts();
                    return Ok(ToolOutput {
                        artifact_type: manifest.artifact_type.clone(),
                        payload: decision.output,
                        sources,
                        warning,
                        model_route: routes.last().cloned(),
                        model_routes: routes,
                        execution_steps: turn,
                        agent_trace: trace,
                    });
                }
                AgentAction::Tool => {
                    validate_tool_decision(&decision)?;
                    if turn == manifest.max_steps {
                        return Err(ToolExecutionError::new(
                            "agent_turn_limit",
                            "agent requested another tool without reserving a final turn",
                        ));
                    }
                    if seen_calls.len() >= MAX_AGENT_TOOL_CALLS {
                        return Err(ToolExecutionError::new(
                            "agent_tool_limit",
                            "agent exceeded the bounded tool-call limit",
                        ));
                    }
                    let reference = CapabilityReference {
                        id: decision.tool_id.clone(),
                        version: decision.tool_version,
                    };
                    let tool_index = manifest
                        .tools
                        .iter()
                        .position(|allowed| allowed == &reference)
                        .ok_or_else(|| {
                            ToolExecutionError::new(
                                "agent_tool_forbidden",
                                format!(
                                    "agent requested unlisted tool '{}@{}'",
                                    reference.id, reference.version
                                ),
                            )
                        })?;
                    let call_key = format!(
                        "{}@{}:{}",
                        reference.id,
                        reference.version,
                        serde_json::to_string(&decision.arguments).unwrap_or_default()
                    );
                    if !seen_calls.insert(call_key) {
                        return Err(ToolExecutionError::new(
                            "duplicate_agent_tool_call",
                            "agent repeated an equivalent tool call",
                        ));
                    }
                    let display_name = self.available_tools[tool_index].display_name.clone();
                    let _ = progress.send(ToolProgress::agent(
                        "agent_calling_read_only_tool",
                        turn,
                        manifest.max_steps,
                        Some((&reference.id, &display_name)),
                    ));
                    let tool_result = self
                        .execute_nested_tool(
                            &reference,
                            decision.arguments.clone(),
                            context.clone(),
                            &progress,
                            turn,
                            &display_name,
                        )
                        .await;
                    evidence.absorb(
                        tool_result,
                        turn,
                        &reference,
                        &display_name,
                        decision.arguments,
                    )?;
                    let _ = progress.send(ToolProgress::agent(
                        "agent_evaluating_tool_result",
                        turn,
                        manifest.max_steps,
                        Some((&reference.id, &display_name)),
                    ));
                }
            }
        }
        Err(ToolExecutionError::new(
            "agent_turn_limit",
            "agent reached its turn limit without a final result",
        ))
    }
}
