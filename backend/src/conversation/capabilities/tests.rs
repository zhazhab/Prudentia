use std::{
    collections::HashMap,
    fs,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::{
    agent_capability::AgentCapabilityTool, manifest::load_capability_manifests,
    model_capability::SkillCapabilityTool, registry::ToolRegistry,
    rule_graph::CapabilityRuleNodeAdapter, *,
};
use crate::{
    ai::{
        cli::{CliProviderKind, CliSettings},
        runtime::{AiProviderKind, AiRuntime, AiSettings, ModelTierSettings},
        ConversationContext,
    },
    conversation::engine::TurnCancellation,
    investment_system::RuleNodeAdapter,
    portfolio::PortfolioSummary,
};

mod planning;

const TEST_SKILL_MANIFEST: &str = r#"{
  "id": "analyze_unit_economics",
  "version": 1,
  "kind": "skill",
  "stage": "analysis",
  "display_name": "Unit economics analysis",
  "description": "Analyze a company's unit economics",
  "artifact_type": "unit_economics_analysis",
  "instructions": "Separate facts, inferences, and unknowns.",
  "input_schema": {
    "type": "object",
    "additionalProperties": false,
    "required": ["focus"],
    "properties": {"focus": {"type": "string", "maxLength": 4000}}
  },
  "output_schema": {
    "type": "object",
    "required": ["summary"],
    "properties": {"summary": {"type": "string"}}
  },
  "context": ["subject", "company_view", "research_sources"],
  "model": "standard",
  "timeout_seconds": 180,
  "max_steps": 1,
  "surfaces": ["conversation"],
  "subjects": ["company"],
  "triggers": ["unit economics"],
  "initial_activity": "skill_analyzing_unit_economics"
}"#;

const TEST_AGENT_MANIFEST: &str = r#"{
  "id": "challenge_unit_economics",
  "version": 1,
  "kind": "agent",
  "stage": "challenge",
  "display_name": "Unit economics challenger",
  "description": "Independently challenge unit economics with bounded research",
  "artifact_type": "unit_economics_challenge",
  "instructions": "Find material evidence gaps, use tools only when needed, and return a falsifiable conclusion.",
  "input_schema": {
    "type": "object",
    "additionalProperties": false,
    "required": ["focus"],
    "properties": {"focus": {"type": "string", "maxLength": 4000}}
  },
  "output_schema": {
    "type": "object",
    "additionalProperties": false,
    "required": ["summary"],
    "properties": {"summary": {"type": "string"}}
  },
  "context": ["subject", "company_view", "research_sources"],
  "model": "deep",
  "timeout_seconds": 300,
  "max_steps": 4,
  "tools": [{"id": "test_read", "version": 1}],
  "skills": [{"id": "analyze_unit_economics", "version": 1}],
  "surfaces": ["conversation"],
  "subjects": ["company"],
  "triggers": ["challenge unit economics"],
  "initial_activity": "agent_challenging_unit_economics"
}"#;

#[test]
fn declarative_skill_manifest_is_validated_and_content_addressed() {
    let definition = parse_capability_manifest(TEST_SKILL_MANIFEST).expect("valid manifest");

    assert_eq!(definition.manifest.id, "analyze_unit_economics");
    assert_eq!(definition.manifest.kind, CapabilityKind::Skill);
    assert_eq!(definition.manifest.model, TaskComplexity::Standard);
    assert_eq!(definition.content_hash.len(), 64);
}

#[test]
fn agent_manifest_declares_exact_tools_skills_and_a_bounded_turn_budget() {
    let definition = parse_capability_manifest(TEST_AGENT_MANIFEST).expect("valid agent manifest");

    assert_eq!(definition.manifest.kind, CapabilityKind::Agent);
    assert_eq!(definition.manifest.stage, CapabilityStage::Challenge);
    assert_eq!(definition.manifest.max_steps, 4);
    assert_eq!(definition.manifest.tools[0].id, "test_read");
    assert_eq!(definition.manifest.skills[0].id, "analyze_unit_economics");
}

#[test]
fn skill_manifest_cannot_smuggle_agent_tools() {
    let invalid = TEST_SKILL_MANIFEST.replace(
        "\"max_steps\": 1",
        "\"max_steps\": 1, \"tools\": [{\"id\": \"test_read\", \"version\": 1}]",
    );

    let error = parse_capability_manifest(&invalid).expect_err("skill tools must fail closed");

    assert_eq!(error.code(), "invalid_capability_manifest");
}

#[test]
fn manifest_rejects_unbounded_or_executable_configuration() {
    let invalid = TEST_SKILL_MANIFEST
        .replace("\"max_steps\": 1", "\"max_steps\": 99")
        .replace(
            "\"initial_activity\": \"skill_analyzing_unit_economics\"",
            "\"initial_activity\": \"skill_analyzing_unit_economics\", \"command\": \"rm -rf /\"",
        );

    let error = parse_capability_manifest(&invalid).expect_err("manifest must fail closed");

    assert_eq!(error.code(), "invalid_capability_manifest");
}

#[test]
fn manifest_loader_keeps_valid_capabilities_when_a_neighbor_is_invalid() {
    let directory = tempfile::tempdir().expect("capability directory");
    fs::write(directory.path().join("valid.json"), TEST_SKILL_MANIFEST)
        .expect("write valid manifest");
    fs::write(
        directory.path().join("invalid.json"),
        TEST_SKILL_MANIFEST.replace(
            "\"initial_activity\": \"skill_analyzing_unit_economics\"",
            "\"initial_activity\": \"skill_analyzing_unit_economics\", \"command\": \"echo unsafe\"",
        ),
    )
    .expect("write invalid manifest");

    let report = load_capability_manifests(directory.path()).expect("load directory");

    assert_eq!(report.definitions.len(), 1);
    assert_eq!(report.definitions[0].manifest.id, "analyze_unit_economics");
    assert_eq!(report.failures.len(), 1);
    assert!(report.failures[0].path.ends_with("invalid.json"));
}

#[test]
fn schema_validation_rejects_missing_required_output() {
    let definition = parse_capability_manifest(TEST_SKILL_MANIFEST).expect("valid manifest");

    let error = validate_json_schema(
        &json!({}),
        &definition.manifest.output_schema,
        "capability output",
    )
    .expect_err("required output is enforced");

    assert_eq!(error.code(), "capability_schema_mismatch");
}

#[test]
fn manifest_rejects_schema_keywords_the_runtime_does_not_enforce() {
    let invalid = TEST_SKILL_MANIFEST.replace(
        "\"output_schema\": {",
        "\"output_schema\": {\"oneOf\": [{\"type\": \"object\"}],",
    );

    let error = parse_capability_manifest(&invalid).expect_err("unsupported schema must fail");

    assert_eq!(error.code(), "invalid_capability_schema");
}

#[test]
fn tool_plan_rejects_more_than_the_bounded_number_of_steps() {
    let calls = (0..=MAX_TOOL_STEPS)
        .map(|index| PlannedToolCall::test_read_call(format!("call-{index}")))
        .collect();

    let error = ToolPlan::new(calls).expect_err("oversized plan must fail closed");

    assert_eq!(error.code(), "tool_plan_too_large");
}

#[test]
fn tool_plan_rejects_duplicate_call_ids() {
    let calls = vec![
        PlannedToolCall::test_read_call("same-call".to_string()),
        PlannedToolCall::test_read_call("same-call".to_string()),
    ];

    let error = ToolPlan::new(calls).expect_err("duplicate ids must fail closed");

    assert_eq!(error.code(), "duplicate_tool_call_id");
}

#[test]
fn explicit_community_requests_use_the_dedicated_tool() {
    let subject = ThreadSubject {
        kind: "company".to_string(),
        subject_key: Some("0700.HK".to_string()),
        label: Some("腾讯控股".to_string()),
        confidence: 0.98,
    };

    let plan = ToolPlan::for_turn("run-community", "看看腾讯的社区观点", &subject, &[])
        .expect("community tool plan");
    let calls = plan.into_calls();

    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].tool_name, RESEARCH_COMMUNITY_INSIGHTS_TOOL);
    assert_eq!(calls[0].call_id, "run-community:tool:1");
    assert_eq!(calls[0].subject_label.as_deref(), Some("腾讯控股"));
    let input: ResearchToolInput =
        serde_json::from_value(calls[0].arguments.clone()).expect("community arguments");
    let serialized_plan = serde_json::to_value(input.plan).expect("serialize community plan");
    assert!(serialized_plan["annual_history_years"].is_null());
}

#[test]
fn company_analysis_with_community_evidence_keeps_broad_research() {
    let subject = ThreadSubject {
        kind: "company".to_string(),
        subject_key: Some("0700.HK".to_string()),
        label: Some("腾讯控股".to_string()),
        confidence: 0.98,
    };

    let plan = ToolPlan::for_turn(
        "run-company-community",
        "分析腾讯商业模式，并补充 Reddit 社区观点",
        &subject,
        &[],
    )
    .expect("combined research plan");

    assert_eq!(plan.into_calls()[0].tool_name, RESEARCH_COMPANY_TOOL);
}

#[test]
fn ordinary_company_research_keeps_the_company_research_tool() {
    let subject = ThreadSubject {
        kind: "company".to_string(),
        subject_key: Some("0700.HK".to_string()),
        label: Some("腾讯控股".to_string()),
        confidence: 0.98,
    };

    let plan = ToolPlan::for_turn("run-company", "分析腾讯的商业模式", &subject, &[])
        .expect("company tool plan");
    let calls = plan.into_calls();

    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].tool_name, RESEARCH_COMPANY_TOOL);
}

#[test]
fn company_request_plans_skill_and_agent_as_distinct_stages() {
    let subject = ThreadSubject {
        kind: "company".to_string(),
        subject_key: Some("0700.HK".to_string()),
        label: Some("腾讯控股".to_string()),
        confidence: 0.98,
    };
    let descriptors = vec![
        planning_descriptor(
            "analyze_business_model",
            CapabilityKind::Skill,
            &["商业模式"],
        ),
        planning_descriptor(
            "challenge_company_thesis",
            CapabilityKind::Agent,
            &["反方分析"],
        ),
    ];

    let plan = ToolPlan::for_turn(
        "run-analysis",
        "分析腾讯的商业模式，并给出反方分析",
        &subject,
        &descriptors,
    )
    .expect("capability plan");
    let calls = plan.into_calls();

    assert_eq!(calls.len(), 3);
    assert_eq!(calls[0].stage, CapabilityStage::Research);
    assert_eq!(calls[1].tool_name, "analyze_business_model");
    assert_eq!(calls[1].stage, CapabilityStage::Analysis);
    assert_eq!(calls[2].tool_name, "challenge_company_thesis");
    assert_eq!(calls[2].stage, CapabilityStage::Challenge);
}

#[test]
fn one_turn_persists_at_most_three_model_capability_outputs() {
    let subject = ThreadSubject {
        kind: "company".to_string(),
        subject_key: Some("0700.HK".to_string()),
        label: Some("腾讯控股".to_string()),
        confidence: 0.98,
    };
    let descriptors = ["alpha", "beta", "gamma", "omega"]
        .map(|name| planning_descriptor(name, CapabilityKind::Skill, &[name]))
        .to_vec();

    let calls = ToolPlan::for_turn(
        "bounded-run",
        "alpha beta gamma omega",
        &subject,
        &descriptors,
    )
    .expect("bounded plan")
    .into_calls();

    assert_eq!(
        calls
            .iter()
            .filter(|call| call.stage != CapabilityStage::Research)
            .count(),
        MAX_MODEL_CAPABILITIES_PER_TURN
    );
}

#[test]
fn capability_subject_scope_allows_future_investment_system_agents() {
    let subject = ThreadSubject {
        kind: "investment_system".to_string(),
        subject_key: Some("default".to_string()),
        label: Some("投资体系".to_string()),
        confidence: 1.0,
    };
    let mut descriptor = planning_descriptor(
        "audit_investment_rules",
        CapabilityKind::Agent,
        &["规则审计"],
    );
    descriptor.subjects = vec![CapabilitySubjectKind::InvestmentSystem];

    let calls = ToolPlan::for_turn(
        "system-run",
        "对当前投资系统做规则审计",
        &subject,
        &[descriptor],
    )
    .expect("system capability plan")
    .into_calls();

    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].tool_name, "audit_investment_rules");
    assert_eq!(calls[0].stage, CapabilityStage::Challenge);
}

#[tokio::test]
async fn registered_read_tool_emits_a_bounded_lifecycle_and_returns_output() {
    let tools = ConversationTools::from_tools([Arc::new(TestTool::read_only()) as Arc<_>])
        .expect("valid registry");
    let plan = ToolPlan::new(vec![PlannedToolCall {
        call_id: "run-1:tool:1".to_string(),
        tool_name: "test_read".to_string(),
        tool_version: 1,
        arguments: json!({ "value": 7 }),
        subject_label: Some("Example Co".to_string()),
        stage: CapabilityStage::Research,
    }])
    .expect("valid plan");
    let cancellation = TurnCancellation::active_for_test();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();

    let report = tools
        .execute(
            plan,
            CapabilityExecutionContext::without_conversation(Locale::En),
            &cancellation,
            event_tx,
        )
        .await
        .expect("execute plan");
    let events = drain_events(&mut event_rx);

    assert_eq!(report.completed.len(), 1);
    assert!(report.failures.is_empty());
    assert_eq!(
        report.completed[0].output.payload,
        json!({ "echo": { "value": 7 } })
    );
    assert_eq!(events.len(), 3);
    assert!(matches!(
        &events[0],
        ToolLifecycleEvent::Started {
            call_id,
            tool_name,
            step_index: 1,
            total_steps: 1,
            subject_label: Some(label),
            ..
        } if call_id == "run-1:tool:1" && tool_name == "test_read" && label == "Example Co"
    ));
    assert!(matches!(events[1], ToolLifecycleEvent::Progress { .. }));
    assert!(matches!(events[2], ToolLifecycleEvent::Completed { .. }));
}

#[tokio::test]
async fn independent_capabilities_in_one_stage_run_concurrently() {
    let active = Arc::new(AtomicUsize::new(0));
    let maximum = Arc::new(AtomicUsize::new(0));
    let tools = ConversationTools::from_tools([
        Arc::new(ConcurrentTool::new(
            "parallel_one",
            active.clone(),
            maximum.clone(),
        )) as Arc<_>,
        Arc::new(ConcurrentTool::new(
            "parallel_two",
            active.clone(),
            maximum.clone(),
        )) as Arc<_>,
    ])
    .expect("valid registry");
    let calls = ["parallel_one", "parallel_two"]
        .into_iter()
        .enumerate()
        .map(|(index, name)| PlannedToolCall {
            call_id: format!("parallel:{index}"),
            tool_name: name.to_string(),
            tool_version: 1,
            arguments: json!({}),
            subject_label: None,
            stage: CapabilityStage::Analysis,
        })
        .collect();
    let plan = ToolPlan::new(calls).expect("parallel plan");
    let (event_tx, _event_rx) = mpsc::unbounded_channel();

    let report = tools
        .execute(
            plan,
            CapabilityExecutionContext::without_conversation(Locale::En),
            &TurnCancellation::active_for_test(),
            event_tx,
        )
        .await
        .expect("execute in parallel");

    assert_eq!(report.completed.len(), 2);
    assert_eq!(maximum.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn agent_runner_loads_skills_and_calls_only_registered_read_tools() {
    let definition = parse_capability_manifest(TEST_AGENT_MANIFEST).expect("agent manifest");
    let skill = parse_capability_manifest(TEST_SKILL_MANIFEST).expect("skill manifest");
    let skill_reference = CapabilityReference {
        id: skill.manifest.id.clone(),
        version: skill.manifest.version,
    };
    let skill_catalog = HashMap::from([(skill_reference, skill)]);
    let invoked = Arc::new(AtomicBool::new(false));
    let mut read_tool = TestTool::read_only();
    read_tool.invoked = invoked.clone();
    let toolbox = Arc::new(
        ToolRegistry::from_tools([Arc::new(read_tool) as Arc<dyn ConversationTool>])
            .expect("native toolbox"),
    );
    let tool = AgentCapabilityTool::new(
        definition,
        Arc::new(mock_ai_runtime()),
        toolbox,
        &skill_catalog,
    )
    .expect("agent dependencies");
    let context = CapabilityExecutionContext::with_conversation(
        Locale::Zh,
        Arc::new(company_context("challenge unit economics")),
    );
    let (progress_tx, mut progress_rx) = mpsc::unbounded_channel();

    let output = tool
        .invoke(
            json!({ "focus": "challenge unit economics" }),
            context,
            progress_tx,
        )
        .await
        .expect("agent output");

    assert!(invoked.load(Ordering::SeqCst));
    assert_eq!(output.artifact_type, "unit_economics_challenge");
    assert_eq!(output.execution_steps, 2);
    assert_eq!(output.agent_trace.len(), 2);
    assert_eq!(output.agent_trace[0].action, "tool");
    assert_eq!(output.agent_trace[0].tool_id.as_deref(), Some("test_read"));
    assert_eq!(output.agent_trace[1].action, "final");
    assert_eq!(
        output
            .model_route
            .as_ref()
            .map(|route| route.provider.as_str()),
        Some("mock")
    );
    assert!(output.payload["summary"].is_string());
    let progress = std::iter::from_fn(|| progress_rx.try_recv().ok())
        .map(|progress| progress.activity)
        .collect::<Vec<_>>();
    assert!(progress.contains(&"agent_planning_next_step".to_string()));
    assert!(progress.contains(&"agent_calling_read_only_tool".to_string()));
    assert!(progress.contains(&"agent_evaluating_tool_result".to_string()));
    assert!(progress.contains(&"agent_synthesizing_result".to_string()));
}

#[test]
fn agent_dependency_resolution_rejects_mutation_tools() {
    let agent =
        parse_capability_manifest(&TEST_AGENT_MANIFEST.replace("test_read", "test_mutation"))
            .expect("agent manifest");
    let skill = parse_capability_manifest(TEST_SKILL_MANIFEST).expect("skill manifest");
    let skill_catalog = HashMap::from([(
        CapabilityReference {
            id: skill.manifest.id.clone(),
            version: skill.manifest.version,
        },
        skill,
    )]);
    let toolbox = Arc::new(
        ToolRegistry::from_tools([
            Arc::new(TestTool::mutation(Arc::new(AtomicBool::new(false))))
                as Arc<dyn ConversationTool>,
        ])
        .expect("toolbox registry"),
    );

    let error =
        AgentCapabilityTool::new(agent, Arc::new(mock_ai_runtime()), toolbox, &skill_catalog)
            .err()
            .expect("mutation dependency must fail closed");

    assert_eq!(error.code(), "agent_tool_forbidden");
}

#[test]
fn agent_dependency_resolution_rejects_incompatible_skill_scope() {
    let agent = parse_capability_manifest(TEST_AGENT_MANIFEST).expect("agent manifest");
    let mut skill = parse_capability_manifest(TEST_SKILL_MANIFEST).expect("skill manifest");
    skill.manifest.subjects = vec![CapabilitySubjectKind::InvestmentSystem];
    let skill_catalog = HashMap::from([(
        CapabilityReference {
            id: skill.manifest.id.clone(),
            version: skill.manifest.version,
        },
        skill,
    )]);
    let toolbox = Arc::new(
        ToolRegistry::from_tools([Arc::new(TestTool::read_only()) as Arc<dyn ConversationTool>])
            .expect("toolbox registry"),
    );

    let error =
        AgentCapabilityTool::new(agent, Arc::new(mock_ai_runtime()), toolbox, &skill_catalog)
            .err()
            .expect("scope mismatch must fail closed");

    assert_eq!(error.code(), "capability_dependency_scope_mismatch");
}

#[test]
fn agent_dependency_resolution_rejects_incompatible_tool_surface() {
    let mut agent = parse_capability_manifest(TEST_AGENT_MANIFEST).expect("agent manifest");
    agent.manifest.surfaces = vec![CapabilitySurface::RuleGraph];
    agent.manifest.context = vec![CapabilityContextKey::RuleGraphInput];
    agent.manifest.skills.clear();
    let toolbox = Arc::new(
        ToolRegistry::from_tools([Arc::new(TestTool::read_only()) as Arc<dyn ConversationTool>])
            .expect("toolbox registry"),
    );

    let error =
        AgentCapabilityTool::new(agent, Arc::new(mock_ai_runtime()), toolbox, &HashMap::new())
            .err()
            .expect("surface mismatch must fail closed");

    assert_eq!(error.code(), "capability_dependency_scope_mismatch");
}

#[test]
fn agent_dependency_resolution_bounds_total_loaded_skill_instructions() {
    let mut agent = parse_capability_manifest(TEST_AGENT_MANIFEST).expect("agent manifest");
    let mut first = parse_capability_manifest(TEST_SKILL_MANIFEST).expect("first skill");
    first.manifest.id = "large_skill_one".to_string();
    first.manifest.instructions = "x".repeat(25_000);
    let mut second = first.clone();
    second.manifest.id = "large_skill_two".to_string();
    agent.manifest.skills = vec![
        CapabilityReference {
            id: first.manifest.id.clone(),
            version: first.manifest.version,
        },
        CapabilityReference {
            id: second.manifest.id.clone(),
            version: second.manifest.version,
        },
    ];
    let skill_catalog = HashMap::from([
        (agent.manifest.skills[0].clone(), first),
        (agent.manifest.skills[1].clone(), second),
    ]);
    let toolbox = Arc::new(
        ToolRegistry::from_tools([Arc::new(TestTool::read_only()) as Arc<dyn ConversationTool>])
            .expect("toolbox registry"),
    );

    let error =
        AgentCapabilityTool::new(agent, Arc::new(mock_ai_runtime()), toolbox, &skill_catalog)
            .err()
            .expect("aggregate skill budget must fail closed");

    assert_eq!(error.code(), "agent_skill_budget_exceeded");
}

#[tokio::test]
async fn rule_graph_adapter_reuses_the_registered_model_capability() {
    let mut definition = parse_capability_manifest(TEST_SKILL_MANIFEST).expect("skill manifest");
    definition.manifest.surfaces = vec![CapabilitySurface::RuleGraph];
    definition
        .manifest
        .context
        .push(CapabilityContextKey::RuleGraphInput);
    let tool = Arc::new(SkillCapabilityTool::new(
        definition,
        Arc::new(mock_ai_runtime()),
    )) as Arc<dyn ConversationTool>;
    let descriptor = tool.descriptor();
    let registry = Arc::new(ToolRegistry::from_tools([tool]).expect("registry"));
    let adapter = CapabilityRuleNodeAdapter::new(registry, descriptor);

    let output = adapter
        .execute(
            json!({ "context": { "gross_margin": 0.42 }, "incoming": [] }),
            &json!({ "arguments": { "focus": "unit economics" }, "locale": "zh-CN" }),
        )
        .await
        .expect("execute adapter");

    assert_eq!(adapter.key(), "analyze_unit_economics@1");
    assert_eq!(output["summary"], "Mock capability result");
}

#[test]
fn explicit_capability_invocation_matches_the_complete_identifier() {
    let definition = parse_capability_manifest(TEST_SKILL_MANIFEST).expect("skill manifest");
    let descriptor = SkillCapabilityTool::new(definition, Arc::new(mock_ai_runtime())).descriptor();

    let exact = explicitly_requested_capabilities("@analyze_unit_economics");
    assert!(capability_requested(
        "@analyze_unit_economics",
        &descriptor,
        &exact
    ));
    let extended = explicitly_requested_capabilities("@analyze_unit_economics_extended");
    assert!(!capability_requested(
        "@analyze_unit_economics_extended",
        &descriptor,
        &extended
    ));
}

#[tokio::test]
async fn mutation_tool_cannot_execute_without_confirmation() {
    let invoked = Arc::new(AtomicBool::new(false));
    let tool = TestTool::mutation(invoked.clone());
    let tools = ConversationTools::from_tools([Arc::new(tool) as Arc<_>]).expect("valid registry");
    let plan = ToolPlan::new(vec![PlannedToolCall {
        call_id: "run-2:tool:1".to_string(),
        tool_name: "test_mutation".to_string(),
        tool_version: 1,
        arguments: json!({}),
        subject_label: None,
        stage: CapabilityStage::Research,
    }])
    .expect("valid plan");
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();

    let report = tools
        .execute(
            plan,
            CapabilityExecutionContext::without_conversation(Locale::En),
            &TurnCancellation::active_for_test(),
            event_tx,
        )
        .await
        .expect("policy rejection is a tool failure, not a run failure");

    assert!(!invoked.load(Ordering::SeqCst));
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].code, "confirmation_required");
    let events = drain_events(&mut event_rx);
    assert!(
        matches!(events.last(), Some(ToolLifecycleEvent::Failed { code, .. }) if code == "confirmation_required")
    );
}

#[tokio::test]
async fn planned_tool_version_must_match_the_registered_adapter() {
    let invoked = Arc::new(AtomicBool::new(false));
    let mut tool = TestTool::read_only();
    tool.invoked = invoked.clone();
    let tools = ConversationTools::from_tools([Arc::new(tool) as Arc<_>]).expect("valid registry");
    let plan = ToolPlan::new(vec![PlannedToolCall {
        call_id: "run-3:tool:1".to_string(),
        tool_name: "test_read".to_string(),
        tool_version: 2,
        arguments: json!({}),
        subject_label: None,
        stage: CapabilityStage::Research,
    }])
    .expect("valid plan");
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();

    let report = tools
        .execute(
            plan,
            CapabilityExecutionContext::without_conversation(Locale::En),
            &TurnCancellation::active_for_test(),
            event_tx,
        )
        .await
        .expect("version rejection is a tool failure, not a run failure");

    assert!(!invoked.load(Ordering::SeqCst));
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].code, "unavailable");
    let events = drain_events(&mut event_rx);
    assert!(matches!(
        events.as_slice(),
        [ToolLifecycleEvent::Failed {
            tool_version: 2,
            code,
            ..
        }] if code == "unavailable"
    ));
}

#[test]
fn duplicate_tool_names_fail_registry_construction() {
    let error = ConversationTools::from_tools([
        Arc::new(TestTool::read_only()) as Arc<_>,
        Arc::new(TestTool::read_only()) as Arc<_>,
    ])
    .err()
    .expect("duplicate names must fail");

    assert_eq!(error.code(), "duplicate_tool");
}

include!("tests/support.rs");
