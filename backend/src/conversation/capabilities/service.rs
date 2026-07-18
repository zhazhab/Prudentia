use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Arc,
};

use sqlx::SqlitePool;
use tokio::sync::mpsc;

use crate::{
    error::AppResult,
    investment_system::{RuleNodeAdapter, RuleNodeAdapterRegistry},
};

#[cfg(test)]
use super::ToolExecutionError;
use super::{
    agent_capability::AgentCapabilityTool,
    builtins::builtin_capabilities,
    manifest::{load_capability_manifests, CapabilityReference},
    model_capability::SkillCapabilityTool,
    orchestrator::ToolOrchestrator,
    registry::ToolRegistry,
    research_community_insights::ResearchCommunityInsightsTool,
    research_company::ResearchCompanyTool,
    rule_graph::CapabilityRuleNodeAdapter,
    CapabilityExecutionContext, CapabilityKind, CapabilitySurface, ConversationTool,
    ToolDescriptor, ToolExecutionReport, ToolLifecycleEvent, ToolPlan,
};
use crate::conversation::{
    engine::TurnCancellation,
    research::WebResearchProvider,
    types::{ConversationCapabilitySummary, ThreadSubject},
};

pub(in crate::conversation) struct ConversationTools {
    orchestrator: ToolOrchestrator,
    descriptors: Vec<ToolDescriptor>,
    rule_node_adapters: RuleNodeAdapterRegistry,
}

impl ConversationTools {
    pub(in crate::conversation) fn new(
        pool: SqlitePool,
        research_provider: Arc<dyn WebResearchProvider>,
        ai: Arc<crate::ai::runtime::AiRuntime>,
        capability_dir: &Path,
    ) -> Self {
        let company_research = Arc::new(ResearchCompanyTool::new(
            pool.clone(),
            research_provider.clone(),
        ));
        let community_insights =
            Arc::new(ResearchCommunityInsightsTool::new(pool, research_provider));
        let native_tools = vec![
            company_research as Arc<dyn ConversationTool>,
            community_insights as Arc<dyn ConversationTool>,
        ];
        let toolbox = Arc::new(
            ToolRegistry::from_tools(native_tools.clone())
                .expect("built-in native tool descriptors must be valid and unique"),
        );
        let mut definitions = builtin_capabilities();
        match load_capability_manifests(capability_dir) {
            Ok(report) => {
                definitions.extend(report.definitions);
                for failure in report.failures {
                    tracing::warn!(
                        path = %failure.path.display(),
                        error = %failure.error,
                        "custom capability manifest was ignored"
                    );
                }
            }
            Err(error) => tracing::warn!(
                path = %capability_dir.display(),
                error = %error,
                "custom capabilities were not loaded"
            ),
        }
        let mut identities = native_tools
            .iter()
            .map(|tool| {
                let descriptor = tool.descriptor();
                (descriptor.name, descriptor.version)
            })
            .collect::<HashSet<_>>();
        let mut unique_definitions = Vec::new();
        for definition in definitions {
            let identity = (definition.manifest.id.clone(), definition.manifest.version);
            if !identities.insert(identity.clone()) {
                tracing::warn!(
                    capability_id = %identity.0,
                    version = identity.1,
                    "duplicate capability manifest ignored"
                );
                continue;
            }
            unique_definitions.push(definition);
        }
        let skill_catalog = unique_definitions
            .iter()
            .filter(|definition| definition.manifest.kind == CapabilityKind::Skill)
            .map(|definition| {
                (
                    CapabilityReference {
                        id: definition.manifest.id.clone(),
                        version: definition.manifest.version,
                    },
                    definition.clone(),
                )
            })
            .collect::<HashMap<_, _>>();
        let mut tools = native_tools;
        for definition in unique_definitions {
            let capability_id = definition.manifest.id.clone();
            let capability_version = definition.manifest.version;
            let tool: Result<Arc<dyn ConversationTool>, _> = match definition.manifest.kind {
                CapabilityKind::Skill => {
                    Ok(Arc::new(SkillCapabilityTool::new(definition, ai.clone())))
                }
                CapabilityKind::Agent => AgentCapabilityTool::new(
                    definition,
                    ai.clone(),
                    toolbox.clone(),
                    &skill_catalog,
                )
                .map(|tool| Arc::new(tool) as Arc<dyn ConversationTool>),
                CapabilityKind::Native => unreachable!("manifests cannot define native tools"),
            };
            match tool {
                Ok(tool) => tools.push(tool),
                Err(error) => tracing::warn!(
                    %capability_id,
                    capability_version,
                    error = %error,
                    "capability with invalid dependencies was ignored"
                ),
            }
        }
        let descriptors = tools
            .iter()
            .map(|tool| tool.descriptor())
            .collect::<Vec<_>>();
        let registry = Arc::new(
            ToolRegistry::from_tools(tools)
                .expect("built-in conversation tool descriptors must be valid and unique"),
        );
        let rule_node_adapters = RuleNodeAdapterRegistry::from_adapters(
            descriptors
                .iter()
                .filter(|descriptor| {
                    descriptor.kind != CapabilityKind::Native
                        && descriptor.surfaces.contains(&CapabilitySurface::RuleGraph)
                })
                .map(|descriptor| {
                    Arc::new(CapabilityRuleNodeAdapter::new(
                        registry.clone(),
                        descriptor.clone(),
                    )) as Arc<dyn RuleNodeAdapter>
                }),
        )
        .expect("capability rule node adapters must be unique");
        Self {
            orchestrator: ToolOrchestrator::new(registry),
            descriptors,
            rule_node_adapters,
        }
    }

    pub(in crate::conversation) fn plan_turn(
        &self,
        run_id: &str,
        message: &str,
        subject: &ThreadSubject,
    ) -> AppResult<ToolPlan> {
        ToolPlan::for_turn(run_id, message, subject, &self.descriptors)
    }

    pub(in crate::conversation) async fn execute(
        &self,
        plan: ToolPlan,
        context: CapabilityExecutionContext,
        cancellation: &TurnCancellation,
        events: mpsc::UnboundedSender<ToolLifecycleEvent>,
    ) -> AppResult<ToolExecutionReport> {
        self.orchestrator
            .execute(plan, context, cancellation, events)
            .await
    }

    pub(in crate::conversation) fn rule_node_adapters(&self) -> &RuleNodeAdapterRegistry {
        &self.rule_node_adapters
    }

    pub(in crate::conversation) fn summaries(&self) -> Vec<ConversationCapabilitySummary> {
        let mut summaries = self
            .descriptors
            .iter()
            .map(|descriptor| ConversationCapabilitySummary {
                id: descriptor.name.clone(),
                version: descriptor.version,
                kind: descriptor.kind.as_str().to_string(),
                stage: descriptor.stage.as_str().to_string(),
                display_name: descriptor.display_name.clone(),
                description: descriptor.description.clone(),
                artifact_type: descriptor.artifact_type.clone(),
                model_tier: descriptor
                    .model
                    .map(|complexity| complexity.as_str().to_string()),
                max_steps: descriptor.max_steps,
                loaded_skills: descriptor
                    .skills
                    .iter()
                    .map(|reference| format!("{}@{}", reference.id, reference.version))
                    .collect(),
                allowed_tools: descriptor
                    .tools
                    .iter()
                    .map(|reference| format!("{}@{}", reference.id, reference.version))
                    .collect(),
                surfaces: descriptor
                    .surfaces
                    .iter()
                    .map(|surface| match surface {
                        CapabilitySurface::Conversation => "conversation".to_string(),
                        CapabilitySurface::RuleGraph => "rule_graph".to_string(),
                    })
                    .collect(),
                subjects: descriptor
                    .subjects
                    .iter()
                    .map(|subject| subject.as_str().to_string())
                    .collect(),
                manifest_hash: descriptor.manifest_hash.clone(),
            })
            .collect::<Vec<_>>();
        summaries.sort_by(|left, right| {
            left.id
                .cmp(&right.id)
                .then(left.version.cmp(&right.version))
        });
        summaries
    }

    #[cfg(test)]
    pub(in crate::conversation) fn from_tools(
        tools: impl IntoIterator<Item = Arc<dyn ConversationTool>>,
    ) -> Result<Self, ToolExecutionError> {
        Ok(Self {
            orchestrator: ToolOrchestrator::new(Arc::new(ToolRegistry::from_tools(tools)?)),
            descriptors: Vec::new(),
            rule_node_adapters: RuleNodeAdapterRegistry::default(),
        })
    }
}
