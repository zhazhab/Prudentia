use super::*;

#[test]
fn lead_agent_preloaded_skills_do_not_run_twice_unless_explicitly_requested() {
    let subject = ThreadSubject {
        kind: "company".to_string(),
        subject_key: Some("0700.HK".to_string()),
        label: Some("腾讯控股".to_string()),
        confidence: 0.98,
    };
    let skill = planning_descriptor(
        "analyze_business_model",
        CapabilityKind::Skill,
        &["商业模式"],
    );
    let mut agent = planning_descriptor("analyze_company", CapabilityKind::Agent, &["深度分析"]);
    agent.stage = CapabilityStage::Analysis;
    agent.skills = vec![CapabilityReference {
        id: skill.name.clone(),
        version: skill.version,
    }];

    let automatic = ToolPlan::for_turn(
        "run-lead-agent",
        "深度分析腾讯的商业模式",
        &subject,
        &[skill.clone(), agent.clone()],
    )
    .expect("lead agent plan")
    .into_calls();
    assert_eq!(automatic.len(), 2);
    assert_eq!(automatic[1].tool_name, "analyze_company");

    let explicit = ToolPlan::for_turn(
        "run-explicit-skill",
        "@analyze_business_model 深度分析腾讯的商业模式",
        &subject,
        &[skill, agent],
    )
    .expect("explicit skill plan")
    .into_calls();
    assert_eq!(explicit.len(), 3);
    assert!(explicit
        .iter()
        .any(|call| call.tool_name == "analyze_business_model"));
    assert!(explicit
        .iter()
        .any(|call| call.tool_name == "analyze_company"));
}
