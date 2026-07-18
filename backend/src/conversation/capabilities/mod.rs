use std::{collections::HashSet, sync::Arc, time::Duration};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::{
    ai::{runtime::TaskComplexity, ConversationContext, ConversationResearchSource},
    error::{AppError, AppResult},
    locale::Locale,
};

use super::{
    research::{
        community_request_requires_company_research, plan_community_insights, plan_research,
        ResearchOutcome, ResearchPlan,
    },
    types::ThreadSubject,
};

use manifest::CapabilityReference;
use planning::{capability_requested, explicitly_requested_capabilities};

mod agent_capability;
mod builtins;
mod manifest;
mod model_capability;
mod orchestrator;
mod planning;
mod registry;
mod research_community_insights;
mod research_company;
mod rule_graph;
mod schema;
mod service;

#[cfg(test)]
use manifest::parse_capability_manifest;
#[cfg(test)]
use schema::validate_json_schema;
pub(super) use service::ConversationTools;

const MAX_TOOL_STEPS: usize = 6;
const MAX_MODEL_CAPABILITIES_PER_TURN: usize = 3;
const RESEARCH_COMPANY_TOOL: &str = "research_company";
const RESEARCH_COMMUNITY_INSIGHTS_TOOL: &str = "research_community_insights";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CapabilityStage {
    Research,
    #[default]
    Analysis,
    Challenge,
}

impl CapabilityStage {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Research => "research",
            Self::Analysis => "analysis",
            Self::Challenge => "challenge",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CapabilityKind {
    Native,
    Skill,
    Agent,
}

impl CapabilityKind {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Skill => "skill",
            Self::Agent => "agent",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CapabilitySurface {
    Conversation,
    RuleGraph,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CapabilityContextKey {
    Subject,
    UserMessage,
    CompanyView,
    ResearchSources,
    Attachments,
    ConversationHistory,
    Portfolio,
    InvestmentSystem,
    RuleGraphInput,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CapabilitySubjectKind {
    Company,
    InvestmentSystem,
    Psychology,
    General,
}

impl CapabilitySubjectKind {
    fn from_thread(subject: super::types::ThreadSubjectKind) -> Self {
        match subject {
            super::types::ThreadSubjectKind::Company => Self::Company,
            super::types::ThreadSubjectKind::InvestmentSystem => Self::InvestmentSystem,
            super::types::ThreadSubjectKind::Psychology => Self::Psychology,
            super::types::ThreadSubjectKind::General | super::types::ThreadSubjectKind::Unknown => {
                Self::General
            }
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Company => "company",
            Self::InvestmentSystem => "investment_system",
            Self::Psychology => "psychology",
            Self::General => "general",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(
    dead_code,
    reason = "the registry models policies before write tools are registered"
)]
pub(super) enum ToolSideEffect {
    ReadOnly,
    ProposesMutation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(
    dead_code,
    reason = "the registry models policies before write tools are registered"
)]
pub(super) enum ToolConfirmation {
    Automatic,
    Required,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(
    dead_code,
    reason = "tool adapters choose their cache policy independently"
)]
pub(super) enum ToolCachePolicy {
    None,
    DailyProviderCache,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(
    dead_code,
    reason = "tool adapters choose their storage policy independently"
)]
pub(super) enum ToolStoragePolicy {
    MetadataOnly,
    SourcesAndSummary,
    StructuredArtifact,
}

#[derive(Clone, Debug)]
pub(super) struct ToolDescriptor {
    pub(super) name: String,
    pub(super) version: u16,
    pub(super) kind: CapabilityKind,
    pub(super) stage: CapabilityStage,
    pub(super) display_name: String,
    pub(super) description: String,
    pub(super) artifact_type: String,
    pub(super) input_schema: Value,
    pub(super) output_schema: Value,
    pub(super) context: Vec<CapabilityContextKey>,
    pub(super) model: Option<TaskComplexity>,
    pub(super) max_steps: u8,
    pub(super) tools: Vec<CapabilityReference>,
    pub(super) skills: Vec<CapabilityReference>,
    pub(super) surfaces: Vec<CapabilitySurface>,
    pub(super) subjects: Vec<CapabilitySubjectKind>,
    pub(super) triggers: Vec<String>,
    pub(super) manifest_hash: String,
    pub(super) side_effect: ToolSideEffect,
    pub(super) confirmation: ToolConfirmation,
    pub(super) cache_policy: ToolCachePolicy,
    pub(super) storage_policy: ToolStoragePolicy,
    pub(super) timeout: Duration,
    pub(super) initial_activity: String,
}

pub(super) struct NativeToolDescriptor<'a> {
    pub(super) name: &'a str,
    pub(super) display_name: &'a str,
    pub(super) description: &'a str,
    pub(super) timeout: Duration,
    pub(super) initial_activity: &'a str,
    pub(super) cache_policy: ToolCachePolicy,
    pub(super) storage_policy: ToolStoragePolicy,
}

impl ToolDescriptor {
    fn native(
        descriptor: NativeToolDescriptor<'_>,
        input_schema: Value,
        output_schema: Value,
    ) -> Self {
        Self {
            name: descriptor.name.to_string(),
            version: 1,
            kind: CapabilityKind::Native,
            stage: CapabilityStage::Research,
            display_name: descriptor.display_name.to_string(),
            description: descriptor.description.to_string(),
            artifact_type: descriptor.name.to_string(),
            input_schema,
            output_schema,
            context: Vec::new(),
            model: None,
            max_steps: 1,
            tools: Vec::new(),
            skills: Vec::new(),
            surfaces: vec![CapabilitySurface::Conversation],
            subjects: vec![CapabilitySubjectKind::Company],
            triggers: Vec::new(),
            manifest_hash: format!("builtin:{}:1", descriptor.name),
            side_effect: ToolSideEffect::ReadOnly,
            confirmation: ToolConfirmation::Automatic,
            cache_policy: descriptor.cache_policy,
            storage_policy: descriptor.storage_policy,
            timeout: descriptor.timeout,
            initial_activity: descriptor.initial_activity.to_string(),
        }
    }
}

#[derive(Clone)]
pub(super) struct CapabilityExecutionContext {
    pub(super) locale: Locale,
    pub(super) conversation: Option<Arc<ConversationContext>>,
    pub(super) rule_graph: Option<Value>,
}

impl CapabilityExecutionContext {
    pub(super) fn without_conversation(locale: Locale) -> Self {
        Self {
            locale,
            conversation: None,
            rule_graph: None,
        }
    }

    pub(super) fn with_conversation(
        locale: Locale,
        conversation: Arc<ConversationContext>,
    ) -> Self {
        Self {
            locale,
            conversation: Some(conversation),
            rule_graph: None,
        }
    }

    pub(super) fn with_rule_graph(locale: Locale, rule_graph: Value) -> Self {
        Self {
            locale,
            conversation: None,
            rule_graph: Some(rule_graph),
        }
    }
}

#[async_trait]
pub(super) trait ConversationTool: Send + Sync {
    fn descriptor(&self) -> ToolDescriptor;

    fn agent_input_schema(&self) -> Option<Value> {
        None
    }

    fn prepare_agent_arguments(
        &self,
        arguments: Value,
        _context: &CapabilityExecutionContext,
    ) -> Result<Value, ToolExecutionError> {
        Ok(arguments)
    }

    async fn invoke(
        &self,
        arguments: Value,
        context: CapabilityExecutionContext,
        progress: mpsc::UnboundedSender<ToolProgress>,
    ) -> Result<ToolOutput, ToolExecutionError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ToolProgress {
    pub(super) activity: String,
    pub(super) detail: Option<ToolProgressDetail>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(super) struct ToolProgressDetail {
    pub(super) nested_tool_name: Option<String>,
    pub(super) nested_tool_display_name: Option<String>,
    pub(super) agent_turn: Option<u8>,
    pub(super) agent_turn_limit: Option<u8>,
}

impl ToolProgress {
    pub(super) fn activity(activity: impl Into<String>) -> Self {
        Self {
            activity: activity.into(),
            detail: None,
        }
    }

    pub(super) fn agent(
        activity: impl Into<String>,
        turn: u8,
        turn_limit: u8,
        nested_tool: Option<(&str, &str)>,
    ) -> Self {
        Self {
            activity: activity.into(),
            detail: Some(ToolProgressDetail {
                nested_tool_name: nested_tool.map(|tool| tool.0.to_string()),
                nested_tool_display_name: nested_tool.map(|tool| tool.1.to_string()),
                agent_turn: Some(turn),
                agent_turn_limit: Some(turn_limit),
            }),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub(super) struct ToolExecutionError {
    code: &'static str,
    message: String,
}

impl ToolExecutionError {
    pub(super) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub(super) fn code(&self) -> &'static str {
        self.code
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub(super) struct ToolPlanError {
    code: &'static str,
    message: String,
}

impl ToolPlanError {
    fn too_large(step_count: usize) -> Self {
        Self {
            code: "tool_plan_too_large",
            message: format!("tool plan has {step_count} steps; the maximum is {MAX_TOOL_STEPS}"),
        }
    }

    fn invalid_call(message: impl Into<String>) -> Self {
        Self {
            code: "invalid_tool_call",
            message: message.into(),
        }
    }

    fn duplicate_call_id(call_id: &str) -> Self {
        Self {
            code: "duplicate_tool_call_id",
            message: format!("tool call id '{call_id}' appears more than once"),
        }
    }

    pub(super) fn code(&self) -> &'static str {
        self.code
    }
}

#[derive(Clone, Debug)]
pub(super) struct PlannedToolCall {
    pub(super) call_id: String,
    pub(super) tool_name: String,
    pub(super) tool_version: u16,
    pub(super) arguments: Value,
    pub(super) subject_label: Option<String>,
    pub(super) stage: CapabilityStage,
}

#[derive(Serialize, Deserialize)]
pub(super) struct ResearchToolInput {
    pub(super) plan: ResearchPlan,
}

impl PlannedToolCall {
    #[cfg(test)]
    fn test_read_call(call_id: String) -> Self {
        Self {
            call_id,
            tool_name: "test_read".to_string(),
            tool_version: 1,
            arguments: serde_json::json!({}),
            subject_label: None,
            stage: CapabilityStage::Research,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct ToolPlan {
    calls: Vec<PlannedToolCall>,
}

impl ToolPlan {
    pub(super) fn empty() -> Self {
        Self { calls: Vec::new() }
    }

    pub(super) fn new(calls: Vec<PlannedToolCall>) -> Result<Self, ToolPlanError> {
        if calls.len() > MAX_TOOL_STEPS {
            return Err(ToolPlanError::too_large(calls.len()));
        }
        let mut call_ids = HashSet::with_capacity(calls.len());
        for call in &calls {
            if call.call_id.trim().is_empty()
                || call.tool_name.trim().is_empty()
                || call.tool_version == 0
            {
                return Err(ToolPlanError::invalid_call(
                    "tool calls require non-empty ids, names, and non-zero versions",
                ));
            }
            if !call.arguments.is_object() {
                return Err(ToolPlanError::invalid_call(
                    "tool call arguments must be a JSON object",
                ));
            }
            if !call_ids.insert(call.call_id.as_str()) {
                return Err(ToolPlanError::duplicate_call_id(&call.call_id));
            }
        }
        Ok(Self { calls })
    }

    fn for_turn(
        run_id: &str,
        message: &str,
        subject: &ThreadSubject,
        descriptors: &[ToolDescriptor],
    ) -> AppResult<Self> {
        let community_plan = plan_community_insights(message, subject);
        let mut calls = match (
            community_plan,
            community_request_requires_company_research(message),
        ) {
            (Some(plan), false) => Self::research_calls(run_id, subject, Some(plan), true)?,
            _ => Self::research_calls(run_id, subject, plan_research(message, subject), false)?,
        };
        let normalized = message.to_lowercase();
        let subject_kind = CapabilitySubjectKind::from_thread(subject.kind_type());
        let mut latest = std::collections::HashMap::<&str, &ToolDescriptor>::new();
        for descriptor in descriptors.iter().filter(|descriptor| {
            descriptor.kind != CapabilityKind::Native
                && descriptor
                    .surfaces
                    .contains(&CapabilitySurface::Conversation)
                && descriptor.subjects.contains(&subject_kind)
        }) {
            latest
                .entry(descriptor.name.as_str())
                .and_modify(|current| {
                    if descriptor.version > current.version {
                        *current = descriptor;
                    }
                })
                .or_insert(descriptor);
        }
        let explicitly_requested = explicitly_requested_capabilities(&normalized);
        let mut matched = latest
            .into_values()
            .filter(|descriptor| {
                capability_requested(&normalized, descriptor, &explicitly_requested)
            })
            .collect::<Vec<_>>();
        let preloaded_by_lead_agents = matched
            .iter()
            .filter(|descriptor| {
                descriptor.kind == CapabilityKind::Agent
                    && descriptor.stage == CapabilityStage::Analysis
            })
            .flat_map(|descriptor| descriptor.skills.iter())
            .map(|reference| (reference.id.as_str(), reference.version))
            .collect::<HashSet<_>>();
        matched.retain(|descriptor| {
            descriptor.kind != CapabilityKind::Skill
                || !preloaded_by_lead_agents
                    .contains(&(descriptor.name.as_str(), descriptor.version))
                || explicitly_requested.contains(descriptor.name.as_str())
        });
        matched.sort_by_key(|descriptor| {
            (
                match descriptor.kind {
                    CapabilityKind::Skill => 0,
                    CapabilityKind::Agent => 1,
                    CapabilityKind::Native => 2,
                },
                descriptor.name.as_str(),
            )
        });
        for descriptor in matched.into_iter().take(MAX_MODEL_CAPABILITIES_PER_TURN) {
            if calls.len() >= MAX_TOOL_STEPS {
                break;
            }
            calls.push(PlannedToolCall {
                call_id: format!("{run_id}:tool:{}", calls.len() + 1),
                tool_name: descriptor.name.clone(),
                tool_version: descriptor.version,
                arguments: serde_json::json!({ "focus": message }),
                subject_label: subject
                    .label
                    .clone()
                    .or_else(|| subject.subject_key.clone()),
                stage: descriptor.stage,
            });
        }
        Self::new(calls).map_err(|error| AppError::internal(format!("{}: {error}", error.code())))
    }

    fn research_calls(
        run_id: &str,
        subject: &ThreadSubject,
        research_plan: Option<ResearchPlan>,
        community_insights: bool,
    ) -> AppResult<Vec<PlannedToolCall>> {
        let Some(research_plan) = research_plan else {
            return Ok(Vec::new());
        };
        let arguments = serde_json::to_value(ResearchToolInput {
            plan: research_plan,
        })?;
        Ok(vec![PlannedToolCall {
            call_id: format!("{run_id}:tool:1"),
            tool_name: if community_insights {
                RESEARCH_COMMUNITY_INSIGHTS_TOOL
            } else {
                RESEARCH_COMPANY_TOOL
            }
            .to_string(),
            tool_version: 1,
            arguments,
            subject_label: subject
                .label
                .clone()
                .or_else(|| subject.subject_key.clone()),
            stage: CapabilityStage::Research,
        }])
    }

    pub(super) fn has_calls(&self) -> bool {
        !self.calls.is_empty()
    }

    pub(super) fn has_stage(&self, stage: CapabilityStage) -> bool {
        self.calls.iter().any(|call| call.stage == stage)
    }

    fn into_calls(self) -> Vec<PlannedToolCall> {
        self.calls
    }

    pub(super) fn for_stage(&self, stages: &[CapabilityStage]) -> Self {
        Self {
            calls: self
                .calls
                .iter()
                .filter(|call| stages.contains(&call.stage))
                .cloned()
                .collect(),
        }
    }
}

fn public_tool_error(code: &str) -> (&'static str, &'static str) {
    match code {
        "tool_timed_out" => ("timeout", "Capability timed out before completing."),
        "capability_model_failed" => (
            "provider_error",
            "The configured AI provider could not complete this capability.",
        ),
        "capability_schema_mismatch"
        | "invalid_tool_output"
        | "capability_output_too_large"
        | "invalid_agent_decision"
        | "agent_turn_limit"
        | "agent_tool_limit"
        | "duplicate_agent_tool_call"
        | "agent_observation_too_large" => (
            "invalid_output",
            "Capability returned an invalid structured result.",
        ),
        "unknown_tool" | "tool_version_mismatch" | "agent_tool_forbidden" => {
            ("unavailable", "The requested capability is unavailable.")
        }
        "tool_confirmation_required" => (
            "confirmation_required",
            "This change requires explicit confirmation before it can run.",
        ),
        _ => ("generic", "Capability did not complete."),
    }
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct CapabilityModelRoute {
    pub(super) step: u8,
    pub(super) provider: String,
    pub(super) model: String,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct AgentExecutionTrace {
    pub(super) turn: u8,
    pub(super) action: String,
    pub(super) tool_id: Option<String>,
    pub(super) tool_version: Option<u16>,
    pub(super) tool_display_name: Option<String>,
    pub(super) status: String,
    pub(super) source_count: usize,
    pub(super) error_code: Option<String>,
}

#[derive(Debug)]
pub(super) struct ToolOutput {
    pub(super) artifact_type: String,
    pub(super) payload: Value,
    pub(super) sources: Vec<ConversationResearchSource>,
    pub(super) warning: Option<String>,
    pub(super) model_route: Option<CapabilityModelRoute>,
    pub(super) model_routes: Vec<CapabilityModelRoute>,
    pub(super) execution_steps: u8,
    pub(super) agent_trace: Vec<AgentExecutionTrace>,
}

impl ToolOutput {
    fn research(artifact_type: &str, outcome: ResearchOutcome) -> Self {
        let ResearchOutcome { sources, warning } = outcome;
        Self {
            artifact_type: artifact_type.to_string(),
            payload: serde_json::json!({
                "source_count": sources.len(),
                "has_warning": warning.is_some()
            }),
            sources,
            warning,
            model_route: None,
            model_routes: Vec::new(),
            execution_steps: 1,
            agent_trace: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub(super) struct CompletedToolCall {
    pub(super) call_id: String,
    pub(super) tool_name: String,
    pub(super) tool_version: u16,
    pub(super) capability_kind: CapabilityKind,
    pub(super) display_name: String,
    pub(super) subject_label: Option<String>,
    pub(super) manifest_hash: String,
    pub(super) duration_ms: u64,
    pub(super) storage_policy: ToolStoragePolicy,
    pub(super) output: ToolOutput,
}

#[derive(Debug)]
pub(super) struct FailedToolCall {
    pub(super) call_id: String,
    pub(super) tool_name: String,
    pub(super) tool_version: u16,
    pub(super) capability_kind: Option<CapabilityKind>,
    pub(super) display_name: Option<String>,
    pub(super) subject_label: Option<String>,
    pub(super) manifest_hash: Option<String>,
    pub(super) duration_ms: u64,
    pub(super) storage_policy: Option<ToolStoragePolicy>,
    pub(super) code: String,
    pub(super) message: String,
}

#[derive(Debug, Default)]
pub(super) struct ToolExecutionReport {
    pub(super) completed: Vec<CompletedToolCall>,
    pub(super) failures: Vec<FailedToolCall>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ToolLifecycleEvent {
    Started {
        call_id: String,
        tool_name: String,
        tool_version: u16,
        capability_kind: CapabilityKind,
        display_name: String,
        stage: CapabilityStage,
        step_index: usize,
        total_steps: usize,
        activity: String,
        subject_label: Option<String>,
        cache_policy: ToolCachePolicy,
        storage_policy: ToolStoragePolicy,
    },
    Progress {
        call_id: String,
        tool_name: String,
        tool_version: u16,
        capability_kind: CapabilityKind,
        display_name: String,
        stage: CapabilityStage,
        step_index: usize,
        total_steps: usize,
        activity: String,
        detail: Option<ToolProgressDetail>,
        subject_label: Option<String>,
    },
    Completed {
        call_id: String,
        tool_name: String,
        tool_version: u16,
        capability_kind: CapabilityKind,
        display_name: String,
        stage: CapabilityStage,
        step_index: usize,
        total_steps: usize,
        duration_ms: u64,
        source_count: usize,
        warning: bool,
    },
    Failed {
        call_id: String,
        tool_name: String,
        tool_version: u16,
        capability_kind: Option<CapabilityKind>,
        display_name: Option<String>,
        stage: CapabilityStage,
        step_index: usize,
        total_steps: usize,
        duration_ms: u64,
        code: String,
        message: String,
    },
}

#[cfg(test)]
mod tests;
