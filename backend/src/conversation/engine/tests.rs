use super::turn_support::{
    action_projection_complexity, action_projection_timeout, action_type_allowed_for_subject,
    response_timeout, should_skip_action_projection,
};
use crate::ai::runtime::TaskComplexity;
use crate::conversation::types::ThreadSubject;

#[test]
fn response_timeout_scales_with_task_complexity() {
    assert_eq!(response_timeout(TaskComplexity::Simple).as_secs(), 90);
    assert_eq!(response_timeout(TaskComplexity::Standard).as_secs(), 240);
    assert_eq!(response_timeout(TaskComplexity::Deep).as_secs(), 600);
}

#[test]
fn deep_turns_allow_a_larger_action_projection_window() {
    assert_eq!(
        action_projection_timeout(TaskComplexity::Simple).as_secs(),
        120
    );
    assert_eq!(
        action_projection_timeout(TaskComplexity::Standard).as_secs(),
        120
    );
    assert_eq!(
        action_projection_timeout(TaskComplexity::Deep).as_secs(),
        300
    );
}

#[test]
fn casual_turns_skip_action_projection() {
    assert!(should_skip_action_projection("你好！", false, false));
    assert!(should_skip_action_projection(
        "What can you do?",
        false,
        false
    ));
}

#[test]
fn material_or_evidence_backed_turns_keep_action_projection() {
    assert!(!should_skip_action_projection(
        "你好，帮我记录买入 100 股。",
        false,
        false
    ));
    assert!(!should_skip_action_projection("你好", true, false));
    assert!(!should_skip_action_projection("你好", false, true));
}

#[test]
fn action_projection_uses_a_faster_tier_except_for_executable_rule_graphs() {
    let mut subject = ThreadSubject {
        kind: "company".to_string(),
        subject_key: Some("PDD".to_string()),
        label: Some("PDD Holdings".to_string()),
        confidence: 1.0,
    };
    assert_eq!(
        action_projection_complexity(&subject),
        TaskComplexity::Standard
    );

    subject.kind = "investment_system".to_string();
    assert_eq!(action_projection_complexity(&subject), TaskComplexity::Deep);
}

#[test]
fn rule_graph_actions_are_confined_to_investment_system_subjects() {
    let mut subject = ThreadSubject {
        kind: "company".to_string(),
        subject_key: Some("PDD".to_string()),
        label: Some("PDD Holdings".to_string()),
        confidence: 1.0,
    };
    assert!(!action_type_allowed_for_subject(
        "rule_graph_patch",
        &subject
    ));
    assert!(action_type_allowed_for_subject(
        "company_view_patch",
        &subject
    ));

    subject.kind = "investment_system".to_string();
    assert!(action_type_allowed_for_subject(
        "rule_graph_patch",
        &subject
    ));
}
