use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::ai::runtime::TaskComplexity;

use super::{
    schema::validate_schema_contract, CapabilityContextKey, CapabilityKind, CapabilitySubjectKind,
    CapabilitySurface, ToolExecutionError,
};

const MAX_MANIFEST_BYTES: usize = 256 * 1024;
const MAX_MANIFEST_FILES: usize = 64;
const MAX_MANIFEST_DEPTH: usize = 4;
const MAX_INSTRUCTIONS_CHARS: usize = 32_000;
const MAX_AGENT_STEPS: u8 = 8;
const MAX_AGENT_TOOLS: usize = 8;
const MAX_AGENT_SKILLS: usize = 4;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub(in crate::conversation) struct CapabilityReference {
    pub(in crate::conversation) id: String,
    pub(in crate::conversation) version: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CapabilityManifest {
    pub(super) id: String,
    pub(super) version: u16,
    pub(super) kind: CapabilityKind,
    #[serde(default)]
    pub(super) stage: super::CapabilityStage,
    pub(super) display_name: String,
    pub(super) description: String,
    pub(super) artifact_type: String,
    pub(super) instructions: String,
    pub(super) input_schema: Value,
    pub(super) output_schema: Value,
    #[serde(default)]
    pub(super) context: Vec<CapabilityContextKey>,
    pub(super) model: TaskComplexity,
    pub(super) timeout_seconds: u64,
    pub(super) max_steps: u8,
    #[serde(default)]
    pub(super) tools: Vec<CapabilityReference>,
    #[serde(default)]
    pub(super) skills: Vec<CapabilityReference>,
    pub(super) surfaces: Vec<CapabilitySurface>,
    pub(super) subjects: Vec<CapabilitySubjectKind>,
    #[serde(default)]
    pub(super) triggers: Vec<String>,
    pub(super) initial_activity: String,
}

#[derive(Clone, Debug)]
pub(super) struct CapabilityDefinition {
    pub(super) manifest: CapabilityManifest,
    pub(super) content_hash: String,
}

pub(super) struct CapabilityLoadFailure {
    pub(super) path: PathBuf,
    pub(super) error: String,
}

pub(super) struct CapabilityLoadReport {
    pub(super) definitions: Vec<CapabilityDefinition>,
    pub(super) failures: Vec<CapabilityLoadFailure>,
}

pub(super) fn parse_capability_manifest(
    content: &str,
) -> Result<CapabilityDefinition, ToolExecutionError> {
    if content.len() > MAX_MANIFEST_BYTES {
        return Err(invalid_manifest("manifest exceeds the 256 KiB limit"));
    }
    let manifest = serde_json::from_str::<CapabilityManifest>(content)
        .map_err(|error| invalid_manifest(format!("manifest JSON is invalid: {error}")))?;
    validate_manifest(&manifest)?;
    validate_schema_contract(&manifest.input_schema, "input_schema")?;
    validate_schema_contract(&manifest.output_schema, "output_schema")?;
    Ok(CapabilityDefinition {
        manifest,
        content_hash: format!("{:x}", Sha256::digest(content.as_bytes())),
    })
}

pub(super) fn load_capability_manifests(
    root: &Path,
) -> Result<CapabilityLoadReport, ToolExecutionError> {
    if !root.exists() {
        return Ok(CapabilityLoadReport {
            definitions: Vec::new(),
            failures: Vec::new(),
        });
    }
    if !root.is_dir() {
        return Err(invalid_manifest(format!(
            "capability path '{}' is not a directory",
            root.display()
        )));
    }
    let mut files = Vec::new();
    collect_manifest_files(root, 0, &mut files)?;
    files.sort();
    let mut definitions = Vec::new();
    let mut failures = Vec::new();
    for path in files {
        let parsed = fs::read_to_string(&path)
            .map_err(|error| format!("failed to read '{}': {error}", path.display()))
            .and_then(|content| {
                parse_capability_manifest(&content).map_err(|error| error.to_string())
            });
        match parsed {
            Ok(definition) => definitions.push(definition),
            Err(error) => failures.push(CapabilityLoadFailure { path, error }),
        }
    }
    Ok(CapabilityLoadReport {
        definitions,
        failures,
    })
}

fn collect_manifest_files(
    directory: &Path,
    depth: usize,
    files: &mut Vec<std::path::PathBuf>,
) -> Result<(), ToolExecutionError> {
    if depth > MAX_MANIFEST_DEPTH {
        return Err(invalid_manifest("capability directory nesting is too deep"));
    }
    let entries = fs::read_dir(directory).map_err(|error| {
        invalid_manifest(format!(
            "failed to read capability directory '{}': {error}",
            directory.display()
        ))
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| invalid_manifest(error.to_string()))?;
        let file_type = entry
            .file_type()
            .map_err(|error| invalid_manifest(error.to_string()))?;
        if file_type.is_symlink() {
            return Err(invalid_manifest(format!(
                "capability path '{}' cannot be a symlink",
                entry.path().display()
            )));
        }
        if file_type.is_dir() {
            collect_manifest_files(&entry.path(), depth + 1, files)?;
        } else if file_type.is_file()
            && entry.path().extension().and_then(|value| value.to_str()) == Some("json")
        {
            files.push(entry.path());
            if files.len() > MAX_MANIFEST_FILES {
                return Err(invalid_manifest("too many capability manifests"));
            }
        }
    }
    Ok(())
}

fn validate_manifest(manifest: &CapabilityManifest) -> Result<(), ToolExecutionError> {
    if !valid_identifier(&manifest.id)
        || !valid_identifier(&manifest.artifact_type)
        || !valid_identifier(&manifest.initial_activity)
    {
        return Err(invalid_manifest(
            "id, artifact_type, and initial_activity must be snake_case identifiers",
        ));
    }
    if manifest.version == 0
        || manifest.display_name.trim().is_empty()
        || manifest.description.trim().is_empty()
        || manifest.instructions.trim().is_empty()
        || manifest.instructions.chars().count() > MAX_INSTRUCTIONS_CHARS
        || !manifest.input_schema.is_object()
        || !manifest.output_schema.is_object()
        || manifest.timeout_seconds == 0
        || manifest.timeout_seconds > 600
        || manifest.max_steps == 0
        || (manifest.kind == CapabilityKind::Agent && manifest.max_steps > MAX_AGENT_STEPS)
        || manifest.surfaces.is_empty()
        || manifest.subjects.is_empty()
    {
        return Err(invalid_manifest(
            "manifest fields exceed their bounded contract",
        ));
    }
    if manifest.kind == CapabilityKind::Native {
        return Err(invalid_manifest(
            "declarative manifests may define only skill or agent capabilities",
        ));
    }
    if manifest.stage == super::CapabilityStage::Research {
        return Err(invalid_manifest(
            "declarative model capabilities may use only analysis or challenge stages",
        ));
    }
    if manifest.kind == CapabilityKind::Skill && manifest.max_steps != 1 {
        return Err(invalid_manifest(
            "skill capabilities must have max_steps = 1",
        ));
    }
    if manifest.kind != CapabilityKind::Agent
        && (!manifest.tools.is_empty() || !manifest.skills.is_empty())
    {
        return Err(invalid_manifest(
            "only agent capabilities may declare tools or skills",
        ));
    }
    if manifest.kind == CapabilityKind::Agent
        && !manifest.tools.is_empty()
        && manifest.max_steps < 2
    {
        return Err(invalid_manifest(
            "agents with tools require at least two steps",
        ));
    }
    if manifest.tools.len() > MAX_AGENT_TOOLS || manifest.skills.len() > MAX_AGENT_SKILLS {
        return Err(invalid_manifest(
            "agent tool or skill references exceed their bounded contract",
        ));
    }
    validate_references(manifest)?;
    if manifest.surfaces.contains(&CapabilitySurface::Conversation) {
        validate_conversation_input_schema(&manifest.input_schema)?;
    }
    let grants_rule_graph_input = manifest
        .context
        .contains(&CapabilityContextKey::RuleGraphInput);
    if manifest.surfaces.contains(&CapabilitySurface::RuleGraph) != grants_rule_graph_input {
        return Err(invalid_manifest(
            "rule_graph surface and rule_graph_input context permission must be declared together",
        ));
    }
    let unique_context = manifest.context.iter().copied().collect::<HashSet<_>>();
    let unique_surfaces = manifest.surfaces.iter().copied().collect::<HashSet<_>>();
    let unique_subjects = manifest.subjects.iter().copied().collect::<HashSet<_>>();
    let unique_triggers = manifest
        .triggers
        .iter()
        .map(|trigger| trigger.trim().to_lowercase())
        .collect::<HashSet<_>>();
    if unique_context.len() != manifest.context.len()
        || unique_surfaces.len() != manifest.surfaces.len()
        || unique_subjects.len() != manifest.subjects.len()
        || unique_triggers.len() != manifest.triggers.len()
    {
        return Err(invalid_manifest("manifest lists cannot contain duplicates"));
    }
    if manifest.triggers.len() > 32
        || manifest
            .triggers
            .iter()
            .any(|trigger| trigger.trim().chars().count() < 2 || trigger.chars().count() > 80)
    {
        return Err(invalid_manifest(
            "capability triggers exceed their bounded contract",
        ));
    }
    Ok(())
}

fn validate_references(manifest: &CapabilityManifest) -> Result<(), ToolExecutionError> {
    let mut identities = HashSet::new();
    for reference in manifest.tools.iter().chain(&manifest.skills) {
        if !valid_identifier(&reference.id) || reference.version == 0 {
            return Err(invalid_manifest(
                "tool and skill references require valid ids and non-zero versions",
            ));
        }
        if reference.id == manifest.id && reference.version == manifest.version {
            return Err(invalid_manifest("a capability cannot reference itself"));
        }
        if !identities.insert((reference.id.as_str(), reference.version)) {
            return Err(invalid_manifest(
                "tool and skill references cannot contain duplicates",
            ));
        }
    }
    Ok(())
}

fn validate_conversation_input_schema(schema: &Value) -> Result<(), ToolExecutionError> {
    let focus = schema
        .get("properties")
        .and_then(Value::as_object)
        .and_then(|properties| properties.get("focus"));
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    if schema.get("type").and_then(Value::as_str) != Some("object")
        || focus
            .and_then(|focus| focus.get("type"))
            .and_then(Value::as_str)
            != Some("string")
        || !required.contains(&"focus")
        || required.iter().any(|field| *field != "focus")
    {
        return Err(invalid_manifest(
            "conversation capabilities must require only a string 'focus' input",
        ));
    }
    Ok(())
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn invalid_manifest(message: impl Into<String>) -> ToolExecutionError {
    ToolExecutionError::new("invalid_capability_manifest", message)
}
