use std::collections::HashSet;

use serde_json::Value;

use crate::ai::{AgentToolObservation, ConversationResearchSource};

use super::super::{
    manifest::CapabilityReference, AgentExecutionTrace, ToolExecutionError, ToolOutput,
};

const MAX_AGENT_OBSERVATION_BYTES: usize = 96 * 1024;
const MAX_OBSERVATION_SOURCES: usize = 12;
const MAX_OBSERVATION_SNIPPET_CHARS: usize = 1_600;

#[derive(Default)]
pub(super) struct AgentEvidence {
    observations: Vec<AgentToolObservation>,
    trace: Vec<AgentExecutionTrace>,
    sources: Vec<ConversationResearchSource>,
    source_urls: HashSet<String>,
    warnings: Vec<String>,
}

impl AgentEvidence {
    pub(super) fn observations(&self) -> Vec<AgentToolObservation> {
        self.observations.clone()
    }

    pub(super) fn source_urls(&self) -> &HashSet<String> {
        &self.source_urls
    }

    pub(super) fn record_final(&mut self, turn: u8) {
        self.trace.push(AgentExecutionTrace {
            turn,
            action: "final".to_string(),
            tool_id: None,
            tool_version: None,
            tool_display_name: None,
            status: "completed".to_string(),
            source_count: self.sources.len(),
            error_code: None,
        });
    }

    pub(super) fn absorb(
        &mut self,
        result: Result<ToolOutput, ToolExecutionError>,
        turn: u8,
        reference: &CapabilityReference,
        display_name: &str,
        arguments: Value,
    ) -> Result<(), ToolExecutionError> {
        match result {
            Ok(output) => self.absorb_success(output, turn, reference, display_name, arguments),
            Err(error) => self.absorb_failure(error, turn, reference, display_name, arguments),
        }
        self.enforce_budget()
    }

    pub(super) fn into_parts(
        self,
    ) -> (
        Vec<ConversationResearchSource>,
        Option<String>,
        Vec<AgentExecutionTrace>,
    ) {
        (
            self.sources,
            (!self.warnings.is_empty()).then(|| self.warnings.join(" ")),
            self.trace,
        )
    }

    fn absorb_success(
        &mut self,
        output: ToolOutput,
        turn: u8,
        reference: &CapabilityReference,
        display_name: &str,
        arguments: Value,
    ) {
        let observation_sources = bounded_sources(&output.sources);
        for source in output.sources {
            if self.source_urls.insert(source.url.clone()) {
                self.sources.push(source);
            }
        }
        if let Some(warning) = output.warning.clone() {
            if !self.warnings.contains(&warning) {
                self.warnings.push(warning);
            }
        }
        self.trace.push(AgentExecutionTrace {
            turn,
            action: "tool".to_string(),
            tool_id: Some(reference.id.clone()),
            tool_version: Some(reference.version),
            tool_display_name: Some(display_name.to_string()),
            status: "completed".to_string(),
            source_count: observation_sources.len(),
            error_code: None,
        });
        self.observations.push(AgentToolObservation {
            turn,
            tool_id: reference.id.clone(),
            tool_version: reference.version,
            arguments,
            status: "completed".to_string(),
            output: output.payload,
            sources: observation_sources,
            warning: output.warning,
            error_code: None,
        });
    }

    fn absorb_failure(
        &mut self,
        error: ToolExecutionError,
        turn: u8,
        reference: &CapabilityReference,
        display_name: &str,
        arguments: Value,
    ) {
        let warning = "One agent research step could not be completed.".to_string();
        if !self.warnings.contains(&warning) {
            self.warnings.push(warning);
        }
        self.trace.push(AgentExecutionTrace {
            turn,
            action: "tool".to_string(),
            tool_id: Some(reference.id.clone()),
            tool_version: Some(reference.version),
            tool_display_name: Some(display_name.to_string()),
            status: "failed".to_string(),
            source_count: 0,
            error_code: Some(error.code().to_string()),
        });
        self.observations.push(AgentToolObservation {
            turn,
            tool_id: reference.id.clone(),
            tool_version: reference.version,
            arguments,
            status: "failed".to_string(),
            output: serde_json::json!({}),
            sources: Vec::new(),
            warning: None,
            error_code: Some(error.code().to_string()),
        });
    }

    fn enforce_budget(&self) -> Result<(), ToolExecutionError> {
        let bytes = serde_json::to_vec(&self.observations).map_err(|error| {
            ToolExecutionError::new("invalid_agent_observation", error.to_string())
        })?;
        if bytes.len() > MAX_AGENT_OBSERVATION_BYTES {
            return Err(ToolExecutionError::new(
                "agent_observation_too_large",
                format!("agent observations exceed {MAX_AGENT_OBSERVATION_BYTES} bytes"),
            ));
        }
        Ok(())
    }
}

fn bounded_sources(sources: &[ConversationResearchSource]) -> Vec<ConversationResearchSource> {
    sources
        .iter()
        .take(MAX_OBSERVATION_SOURCES)
        .map(|source| ConversationResearchSource {
            title: source.title.clone(),
            url: source.url.clone(),
            snippet: source
                .snippet
                .chars()
                .take(MAX_OBSERVATION_SNIPPET_CHARS)
                .collect(),
            source_tier: source.source_tier.clone(),
        })
        .collect()
}
