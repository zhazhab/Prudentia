use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use serde_json::Value;
use tokio::{sync::mpsc, time::timeout};

use crate::ai::AgentModelTool;

use super::{
    manifest::CapabilityReference, schema::validate_json_schema, CapabilityExecutionContext,
    CapabilityStage, CapabilitySubjectKind, CapabilitySurface, ConversationTool, PlannedToolCall,
    ToolConfirmation, ToolDescriptor, ToolExecutionError, ToolOutput, ToolProgress, ToolSideEffect,
};

const MAX_CAPABILITY_OUTPUT_BYTES: usize = 48 * 1024;

pub(super) struct ToolRegistry {
    tools: HashMap<(String, u16), Arc<dyn ConversationTool>>,
}

impl ToolRegistry {
    pub(super) fn from_tools(
        tools: impl IntoIterator<Item = Arc<dyn ConversationTool>>,
    ) -> Result<Self, ToolExecutionError> {
        let mut registered = HashMap::new();
        for tool in tools {
            let descriptor = tool.descriptor();
            validate_descriptor(&descriptor)?;
            let key = (descriptor.name.clone(), descriptor.version);
            if registered.insert(key, tool).is_some() {
                return Err(ToolExecutionError::new(
                    "duplicate_tool",
                    format!("tool '{}' is registered more than once", descriptor.name),
                ));
            }
        }
        Ok(Self { tools: registered })
    }

    pub(super) fn descriptor(
        &self,
        call: &PlannedToolCall,
    ) -> Result<ToolDescriptor, ToolExecutionError> {
        if let Some(tool) = self.tools.get(&(call.tool_name.clone(), call.tool_version)) {
            return Ok(tool.descriptor());
        }
        let mut versions = self
            .tools
            .keys()
            .filter_map(|(name, version)| (name == &call.tool_name).then_some(*version))
            .collect::<Vec<_>>();
        versions.sort_unstable();
        if versions.is_empty() {
            Err(ToolExecutionError::new(
                "unknown_tool",
                format!("tool '{}' is not registered", call.tool_name),
            ))
        } else {
            Err(ToolExecutionError::new(
                "tool_version_mismatch",
                format!(
                    "tool '{}' requested version {} but registry provides {:?}",
                    call.tool_name, call.tool_version, versions
                ),
            ))
        }
    }

    pub(super) fn agent_tool_spec(
        &self,
        reference: &CapabilityReference,
        required_surfaces: &[CapabilitySurface],
        required_subjects: &[CapabilitySubjectKind],
    ) -> Result<AgentModelTool, ToolExecutionError> {
        let (tool, descriptor) = self.tool_by_identity(&reference.id, reference.version)?;
        if descriptor.kind != super::CapabilityKind::Native
            || descriptor.side_effect != ToolSideEffect::ReadOnly
            || descriptor.confirmation != ToolConfirmation::Automatic
        {
            return Err(ToolExecutionError::new(
                "agent_tool_forbidden",
                format!(
                    "capability '{}@{}' is not an automatic read-only native tool",
                    reference.id, reference.version
                ),
            ));
        }
        if !required_surfaces
            .iter()
            .all(|surface| descriptor.surfaces.contains(surface))
            || !required_subjects
                .iter()
                .all(|subject| descriptor.subjects.contains(subject))
        {
            return Err(ToolExecutionError::new(
                "capability_dependency_scope_mismatch",
                format!(
                    "tool '{}@{}' does not cover every agent surface and subject",
                    reference.id, reference.version
                ),
            ));
        }
        let input_schema = tool.agent_input_schema().ok_or_else(|| {
            ToolExecutionError::new(
                "agent_tool_forbidden",
                format!(
                    "tool '{}@{}' is not exposed to model agents",
                    reference.id, reference.version
                ),
            )
        })?;
        crate::json_schema::validate_schema_contract(&input_schema, "agent tool input schema")
            .map_err(|message| ToolExecutionError::new("invalid_tool_descriptor", message))?;
        Ok(AgentModelTool {
            id: descriptor.name,
            version: descriptor.version,
            display_name: descriptor.display_name,
            description: descriptor.description,
            input_schema,
        })
    }

    pub(super) async fn execute_agent_tool(
        &self,
        reference: &CapabilityReference,
        arguments: Value,
        context: CapabilityExecutionContext,
        progress: mpsc::UnboundedSender<ToolProgress>,
    ) -> Result<ToolOutput, ToolExecutionError> {
        let (tool, descriptor) = self.tool_by_identity(&reference.id, reference.version)?;
        let agent_schema = tool.agent_input_schema().ok_or_else(|| {
            ToolExecutionError::new(
                "agent_tool_forbidden",
                format!("tool '{}' is not exposed to model agents", reference.id),
            )
        })?;
        validate_json_schema(&arguments, &agent_schema, "agent tool input")?;
        let prepared = tool.prepare_agent_arguments(arguments, &context)?;
        execute_checked(tool, descriptor, prepared, context, progress).await
    }

    pub(super) async fn execute(
        &self,
        call: &PlannedToolCall,
        context: CapabilityExecutionContext,
        progress: mpsc::UnboundedSender<ToolProgress>,
    ) -> Result<ToolOutput, ToolExecutionError> {
        let (tool, descriptor) = self.tool_by_identity(&call.tool_name, call.tool_version)?;
        execute_checked(tool, descriptor, call.arguments.clone(), context, progress).await
    }

    fn tool_by_identity(
        &self,
        name: &str,
        version: u16,
    ) -> Result<(&Arc<dyn ConversationTool>, ToolDescriptor), ToolExecutionError> {
        let call = PlannedToolCall {
            call_id: "registry-lookup".to_string(),
            tool_name: name.to_string(),
            tool_version: version,
            arguments: serde_json::json!({}),
            subject_label: None,
            stage: super::CapabilityStage::Analysis,
        };
        let descriptor = self.descriptor(&call)?;
        let tool = self
            .tools
            .get(&(name.to_string(), version))
            .expect("descriptor lookup verified registered tool");
        Ok((tool, descriptor))
    }
}

async fn execute_checked(
    tool: &Arc<dyn ConversationTool>,
    descriptor: ToolDescriptor,
    arguments: Value,
    context: CapabilityExecutionContext,
    progress: mpsc::UnboundedSender<ToolProgress>,
) -> Result<ToolOutput, ToolExecutionError> {
    if descriptor.side_effect != ToolSideEffect::ReadOnly
        || descriptor.confirmation != ToolConfirmation::Automatic
    {
        return Err(ToolExecutionError::new(
            "tool_confirmation_required",
            format!(
                "tool '{}' cannot execute before user confirmation",
                descriptor.name
            ),
        ));
    }
    validate_json_schema(&arguments, &descriptor.input_schema, "capability input")?;
    timeout(
        descriptor.timeout,
        tool.invoke(arguments, context, progress),
    )
    .await
    .map_err(|_| {
        ToolExecutionError::new(
            "tool_timed_out",
            format!(
                "tool '{}' timed out after {} seconds",
                descriptor.name,
                descriptor.timeout.as_secs()
            ),
        )
    })?
    .and_then(|output| validate_output(&descriptor, output))
}

fn validate_output(
    descriptor: &ToolDescriptor,
    output: ToolOutput,
) -> Result<ToolOutput, ToolExecutionError> {
    validate_json_schema(
        &output.payload,
        &descriptor.output_schema,
        "capability output",
    )?;
    let output_size = serde_json::to_vec(&output.payload)
        .map_err(|error| ToolExecutionError::new("invalid_tool_output", error.to_string()))?
        .len();
    if output_size > MAX_CAPABILITY_OUTPUT_BYTES {
        return Err(ToolExecutionError::new(
            "capability_output_too_large",
            format!(
                "tool '{}' returned {output_size} bytes; the limit is {MAX_CAPABILITY_OUTPUT_BYTES}",
                descriptor.name
            ),
        ));
    }
    Ok(output)
}

fn validate_descriptor(descriptor: &ToolDescriptor) -> Result<(), ToolExecutionError> {
    let unique_context = descriptor.context.iter().copied().collect::<HashSet<_>>();
    let unique_subjects = descriptor.subjects.iter().copied().collect::<HashSet<_>>();
    let model_contract_valid = match descriptor.kind {
        super::CapabilityKind::Native => {
            descriptor.model.is_none() && descriptor.stage == CapabilityStage::Research
        }
        super::CapabilityKind::Skill | super::CapabilityKind::Agent => {
            descriptor.model.is_some() && descriptor.stage != CapabilityStage::Research
        }
    };
    if descriptor.name.trim().is_empty()
        || descriptor.version == 0
        || descriptor.display_name.trim().is_empty()
        || descriptor.description.trim().is_empty()
        || descriptor.timeout.is_zero()
        || descriptor.initial_activity.trim().is_empty()
        || !descriptor.input_schema.is_object()
        || !descriptor.output_schema.is_object()
        || descriptor.max_steps == 0
        || descriptor.tools.iter().collect::<HashSet<_>>().len() != descriptor.tools.len()
        || descriptor.skills.iter().collect::<HashSet<_>>().len() != descriptor.skills.len()
        || descriptor.surfaces.is_empty()
        || descriptor.subjects.is_empty()
        || descriptor.manifest_hash.trim().is_empty()
        || unique_context.len() != descriptor.context.len()
        || unique_subjects.len() != descriptor.subjects.len()
        || !model_contract_valid
    {
        return Err(ToolExecutionError::new(
            "invalid_tool_descriptor",
            format!("tool '{}' has an invalid descriptor", descriptor.name),
        ));
    }
    Ok(())
}
