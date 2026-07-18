use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::{investment_system::RuleNodeAdapter, locale::Locale};

use super::{
    registry::ToolRegistry, CapabilityExecutionContext, CapabilityKind, CapabilityStage,
    PlannedToolCall, ToolDescriptor,
};

pub(super) struct CapabilityRuleNodeAdapter {
    key: String,
    registry: Arc<ToolRegistry>,
    descriptor: ToolDescriptor,
}

impl CapabilityRuleNodeAdapter {
    pub(super) fn new(registry: Arc<ToolRegistry>, descriptor: ToolDescriptor) -> Self {
        Self {
            key: format!("{}@{}", descriptor.name, descriptor.version),
            registry,
            descriptor,
        }
    }
}

#[async_trait]
impl RuleNodeAdapter for CapabilityRuleNodeAdapter {
    fn key(&self) -> &str {
        &self.key
    }

    fn kind(&self) -> &str {
        self.descriptor.kind.as_str()
    }

    fn manifest_hash(&self) -> Option<&str> {
        Some(&self.descriptor.manifest_hash)
    }

    fn validate_config(&self, config: &Value) -> Result<(), String> {
        let arguments = config
            .get("arguments")
            .ok_or_else(|| "config.arguments is required".to_string())?;
        crate::json_schema::validate_json_schema(
            arguments,
            &self.descriptor.input_schema,
            "rule capability arguments",
        )
    }

    async fn execute(&self, input: Value, config: &Value) -> Result<Value, String> {
        let arguments = config
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let locale = config
            .get("locale")
            .and_then(Value::as_str)
            .map(Locale::from_accept_language)
            .unwrap_or(Locale::Zh);
        let call = PlannedToolCall {
            call_id: format!("rule_graph:{}", self.key),
            tool_name: self.descriptor.name.clone(),
            tool_version: self.descriptor.version,
            arguments,
            subject_label: None,
            stage: match self.descriptor.kind {
                CapabilityKind::Agent => CapabilityStage::Challenge,
                CapabilityKind::Skill | CapabilityKind::Native => CapabilityStage::Analysis,
            },
        };
        let (progress, _progress_rx) = mpsc::unbounded_channel();
        self.registry
            .execute(
                &call,
                CapabilityExecutionContext::with_rule_graph(
                    locale,
                    json!({ "node_input": input, "node_config": config }),
                ),
                progress,
            )
            .await
            .map(|output| output.payload)
            .map_err(|error| format!("{}: {error}", error.code()))
    }
}
