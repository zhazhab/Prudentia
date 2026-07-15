use serde::Deserialize;

use crate::ai::{AiError, ConversationActionDraft, ConversationProjection};

#[derive(Debug, Deserialize)]
pub(super) struct CliConversationProjection {
    summary: String,
    actions: Vec<CliConversationActionDraft>,
}

#[derive(Debug, Deserialize)]
struct CliConversationActionDraft {
    action_type: String,
    title: String,
    rationale: String,
    payload: String,
}

impl TryFrom<CliConversationProjection> for ConversationProjection {
    type Error = AiError;

    fn try_from(value: CliConversationProjection) -> Result<Self, Self::Error> {
        let actions = value
            .actions
            .into_iter()
            .map(|draft| {
                let payload = serde_json::from_str(&draft.payload).map_err(|error| {
                    AiError::Provider(format!(
                        "conversation action payload is not valid JSON: {error}"
                    ))
                })?;
                Ok(ConversationActionDraft {
                    action_type: draft.action_type,
                    title: draft.title,
                    rationale: draft.rationale,
                    payload,
                })
            })
            .collect::<Result<Vec<_>, AiError>>()?;
        Ok(Self {
            summary: value.summary,
            actions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_json_encoded_action_payload() {
        let wire: CliConversationProjection = serde_json::from_value(serde_json::json!({
            "summary": "new conclusion",
            "actions": [{
                "action_type": "company_view_patch",
                "title": "Update risk",
                "rationale": "The discussion established a durable risk.",
                "payload": "{\"symbol\":\"TEST\",\"changes\":{\"risks\":\"Channel concentration\"}}"
            }]
        }))
        .expect("wire projection parses");

        let projection = ConversationProjection::try_from(wire).expect("payload decodes");

        assert_eq!(projection.actions.len(), 1);
        assert_eq!(projection.actions[0].payload["symbol"], "TEST");
    }
}
