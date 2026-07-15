use serde_json::Value;

use crate::ai::ConversationSubjectClarification;

pub(super) fn response_structure(
    clarification: Option<&ConversationSubjectClarification>,
) -> Option<String> {
    clarification.map(|_| {
        "Ask exactly one concise question that requests the company full name or security code. If candidates are supplied, list only those candidates. Do not analyze a company, answer the substantive request, or infer which candidate the user means."
            .to_string()
    })
}

pub(super) fn response_context(
    user_message: &str,
    clarification: Option<&ConversationSubjectClarification>,
) -> Option<Value> {
    clarification.map(|clarification| {
        serde_json::json!({
            "user_message": user_message,
            "subject_clarification": clarification,
        })
    })
}

#[cfg(test)]
mod tests {
    use crate::ai::{ConversationSubjectCandidate, ConversationSubjectClarification};

    use super::{response_context, response_structure};

    #[test]
    fn clarification_prompt_data_contains_only_the_request_and_candidates() {
        let clarification = ConversationSubjectClarification {
            target_hint: Some("平安".to_string()),
            candidates: vec![
                ConversationSubjectCandidate {
                    symbol: "601318.SS".to_string(),
                    name: "中国平安".to_string(),
                },
                ConversationSubjectCandidate {
                    symbol: "2318.HK".to_string(),
                    name: "中国平安".to_string(),
                },
            ],
        };

        let context =
            response_context("分析一下平安", Some(&clarification)).expect("clarification context");
        let serialized = context.to_string();

        assert!(response_structure(Some(&clarification))
            .expect("response structure")
            .contains("Ask exactly one concise question"));
        assert!(serialized.contains("601318.SS"));
        assert!(serialized.contains("2318.HK"));
        assert_eq!(context.as_object().expect("object").len(), 2);
    }
}
