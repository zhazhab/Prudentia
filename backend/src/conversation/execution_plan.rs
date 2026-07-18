use super::{
    research::company_research_scope,
    types::{
        ConversationExecutionPlan, ConversationExecutionPlanStep, ThreadSubject, ThreadSubjectKind,
    },
};

const DEFAULT_COMPANY_TEMPLATE: &[&str] = &[
    "business_model",
    "owner_economics",
    "competitive_position",
    "moat",
    "management_capital_allocation",
    "financial_resilience",
    "earning_power",
    "failure_mechanism",
];

pub(super) fn build_company_execution_plan(
    message: &str,
    subject: &ThreadSubject,
    has_research: bool,
) -> Option<ConversationExecutionPlan> {
    (subject.kind_type() == ThreadSubjectKind::Company && !super::is_simple_social_turn(message))
        .then(|| ConversationExecutionPlan {
            template_id: "company_analysis_v1".to_string(),
            scope: company_research_scope(message).to_string(),
            dimensions: DEFAULT_COMPANY_TEMPLATE
                .iter()
                .map(|dimension| (*dimension).to_string())
                .collect(),
            steps: [
                ("scope", "completed"),
                ("research", if has_research { "pending" } else { "skipped" }),
                ("evidence_baseline", "pending"),
                ("analysis", "pending"),
                ("challenge", "pending"),
                ("synthesis", "pending"),
                ("memo_update", "pending"),
            ]
            .into_iter()
            .map(|(id, status)| ConversationExecutionPlanStep {
                id: id.to_string(),
                status: status.to_string(),
            })
            .collect(),
        })
}

#[cfg(test)]
mod tests {
    use super::build_company_execution_plan;
    use crate::conversation::types::ThreadSubject;

    #[test]
    fn generic_company_request_uses_the_visible_default_template() {
        let plan = build_company_execution_plan(
            "分析 Netflix",
            &ThreadSubject {
                kind: "company".to_string(),
                subject_key: Some("NFLX".to_string()),
                label: Some("Netflix".to_string()),
                confidence: 0.95,
            },
            true,
        )
        .expect("company plan");

        assert_eq!(plan.scope, "default");
        assert_eq!(plan.template_id, "company_analysis_v1");
        assert_eq!(plan.dimensions.len(), 8);
        assert_eq!(plan.steps[0].status, "completed");
        assert_eq!(plan.steps[1].status, "pending");
    }
}
