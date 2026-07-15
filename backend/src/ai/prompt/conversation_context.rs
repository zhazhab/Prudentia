use serde_json::Value;

use crate::ai::ConversationContext;

use super::subject_clarification;

pub(super) fn response(context: &ConversationContext) -> Value {
    if let Some(context) = subject_clarification::response_context(
        &context.user_message,
        context.subject_clarification.as_ref(),
    ) {
        return context;
    }
    if !is_company(context) {
        return serde_json::to_value(context).expect("conversation context serializes");
    }
    let used_context = context
        .used_context
        .iter()
        .filter(|entry| {
            !matches!(
                entry.get("kind").and_then(Value::as_str),
                Some("portfolio" | "investment_system")
            )
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "thread_title": &context.thread_title,
        "thread_summary": &context.thread_summary,
        "turn_summaries": &context.turn_summaries,
        "subject": &context.subject,
        "user_message": &context.user_message,
        "recent_messages": &context.recent_messages,
        "company_view": company_view(context),
        "attachments": &context.attachments,
        "research_sources": &context.research_sources,
        "research_warning": &context.research_warning,
        "used_context": used_context,
    })
}

pub(super) fn projection(context: &ConversationContext) -> Value {
    if let Some(context) = subject_clarification::response_context(
        &context.user_message,
        context.subject_clarification.as_ref(),
    ) {
        return context;
    }
    if is_company(context) {
        return serde_json::json!({
            "subject": &context.subject,
            "user_message": &context.user_message,
            "company_view": company_view(context),
            "attachments": &context.attachments,
        });
    }
    serde_json::json!({
        "subject": &context.subject,
        "user_message": &context.user_message,
        "company_view": &context.company_view,
        "recent_trades": &context.recent_trades,
        "investment_system": &context.investment_system,
        "attachments": &context.attachments,
    })
}

fn is_company(context: &ConversationContext) -> bool {
    context.subject.get("kind").and_then(Value::as_str) == Some("company")
}

fn company_view(context: &ConversationContext) -> Option<Value> {
    context.company_view.clone().map(|mut view| {
        if let Some(object) = view.as_object_mut() {
            object.remove("valuation_expectations");
            if let Some(content) = object.get_mut("content").and_then(Value::as_object_mut) {
                content.remove("valuation_expectations");
            }
        }
        view
    })
}
