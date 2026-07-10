use std::path::Path;

use serde_json::json;
use sqlx::{Row, SqlitePool};

use crate::{
    ai::{ConversationContext, ConversationResearchSource, MemoChatHistoryMessage},
    error::AppResult,
    investment_system::{active_rule_graph, legacy_system},
    memo_thread::{self, MemoThreadMessageRole},
    portfolio,
};

use super::{
    attachments::load_attachment_contexts, company::load_company_view, storage,
    types::ThreadSubject,
};

pub struct ConversationResearchContext {
    pub sources: Vec<ConversationResearchSource>,
    pub warning: Option<String>,
}

pub async fn resolve_subject(
    pool: &SqlitePool,
    thread_id: &str,
    message: &str,
) -> AppResult<ThreadSubject> {
    let current = storage::thread_subject(pool, thread_id).await?;
    if current.kind != "general" {
        return Ok(current);
    }
    let normalized = message.to_ascii_lowercase();
    let subject = if contains_any(
        &normalized,
        &[
            "投资体系",
            "规则图",
            "买入规则",
            "卖出规则",
            "investment system",
            "rule graph",
        ],
    ) {
        ThreadSubject {
            kind: "investment_system".to_string(),
            subject_key: Some("default".to_string()),
            label: Some("投资体系".to_string()),
            confidence: 0.92,
        }
    } else if contains_any(
        &normalized,
        &[
            "心态",
            "情绪",
            "焦虑",
            "后悔",
            "冲动",
            "恐惧",
            "贪婪",
            "psychology",
            "emotion",
        ],
    ) {
        ThreadSubject {
            kind: "psychology".to_string(),
            subject_key: None,
            label: Some("投资心理".to_string()),
            confidence: 0.88,
        }
    } else if let Some((symbol, name)) = match_company(pool, message).await? {
        ThreadSubject {
            kind: "company".to_string(),
            subject_key: Some(symbol),
            label: Some(name),
            confidence: 0.95,
        }
    } else {
        current
    };
    if subject != ThreadSubject::default() {
        storage::update_thread_subject(pool, thread_id, subject.clone()).await?;
    }
    Ok(subject)
}

pub async fn assemble_context(
    pool: &SqlitePool,
    workspace_dir: &Path,
    run_id: &str,
    thread_id: &str,
    user_message: &str,
    subject: &ThreadSubject,
    research: ConversationResearchContext,
) -> AppResult<ConversationContext> {
    let ConversationResearchContext {
        sources: research_sources,
        warning: research_warning,
    } = research;
    let detail = memo_thread::get_detail(pool, thread_id, 20, None).await?;
    let portfolio_positions = portfolio::list_positions(pool).await?;
    let portfolio_summary = portfolio::summary(pool).await?;
    let company_view = match subject
        .subject_key
        .as_deref()
        .filter(|_| subject.kind == "company")
    {
        Some(symbol) => load_company_view(pool, symbol)
            .await?
            .map(serde_json::to_value)
            .transpose()?,
        None => None,
    };
    let recent_trades = match subject
        .subject_key
        .as_deref()
        .filter(|_| subject.kind == "company")
    {
        Some(symbol) => portfolio::recent_trade_events(pool, symbol, 20)
            .await?
            .into_iter()
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()?,
        None => Vec::new(),
    };
    let active_graph = active_rule_graph(pool).await?;
    let legacy = legacy_system(pool).await?;
    let attachments = load_attachment_contexts(pool, workspace_dir, run_id).await?;
    let mut turn_summaries = sqlx::query_scalar::<_, String>(
        r#"SELECT summary FROM conversation_turn_summaries
        WHERE thread_id = ? ORDER BY created_at DESC LIMIT 8"#,
    )
    .bind(thread_id)
    .fetch_all(pool)
    .await?;
    turn_summaries.reverse();
    let recent_messages = detail
        .messages
        .into_iter()
        .filter(|message| {
            !(message.role == MemoThreadMessageRole::Assistant && message.content.is_empty())
        })
        .map(|message| MemoChatHistoryMessage {
            role: role_name(message.role).to_string(),
            content: message.content,
        })
        .collect::<Vec<_>>();
    let mut used_context = vec![
        json!({ "kind": "thread_summary", "label": detail.thread.title }),
        json!({ "kind": "turn_summaries", "label": format!("{} prior turns", turn_summaries.len()) }),
        json!({ "kind": "portfolio", "label": format!("{} positions", portfolio_summary.positions_count) }),
        json!({ "kind": "investment_system", "label": format!("rule graph v{}", active_graph.version) }),
    ];
    if let Some(symbol) = &subject.subject_key {
        used_context.push(json!({ "kind": "company", "label": symbol }));
    }
    for attachment in &attachments {
        used_context.push(json!({
            "kind": "attachment",
            "label": attachment.file_name,
            "status": attachment.parse_status
        }));
    }
    for source in &research_sources {
        used_context.push(json!({ "kind": "source", "label": source.title, "url": source.url }));
    }

    Ok(ConversationContext {
        thread_title: detail.thread.title,
        thread_summary: detail.thread.summary,
        turn_summaries,
        subject: serde_json::to_value(subject)?,
        user_message: user_message.to_string(),
        recent_messages,
        portfolio_summary,
        portfolio_positions,
        company_view,
        recent_trades,
        investment_system: json!({ "active_graph": active_graph, "legacy_reference": legacy }),
        attachments,
        research_sources,
        research_warning,
        used_context,
    })
}

async fn match_company(pool: &SqlitePool, message: &str) -> AppResult<Option<(String, String)>> {
    let upper = message.to_ascii_uppercase();
    let lower = message.to_lowercase();
    let position_rows = sqlx::query("SELECT symbol, name FROM portfolio_positions")
        .fetch_all(pool)
        .await?;
    let mut matches = position_rows
        .into_iter()
        .filter_map(|row| {
            let symbol: String = row.try_get("symbol").ok()?;
            let name: String = row.try_get("name").ok()?;
            (upper.contains(&symbol.to_ascii_uppercase()) || lower.contains(&name.to_lowercase()))
                .then_some((symbol, name))
        })
        .collect::<Vec<_>>();
    if matches.len() == 1 {
        return Ok(matches.pop());
    }
    let directory = sqlx::query(
        r#"SELECT symbol, name FROM security_symbols
        WHERE length(name) >= 2 AND (
            lower(?) LIKE '%' || lower(name) || '%' OR upper(?) LIKE '%' || upper(symbol) || '%'
        ) ORDER BY length(name) DESC LIMIT 2"#,
    )
    .bind(message)
    .bind(message)
    .fetch_all(pool)
    .await?;
    if directory.len() == 1 {
        let row = &directory[0];
        return Ok(Some((row.try_get("symbol")?, row.try_get("name")?)));
    }
    Ok(None)
}

fn contains_any(value: &str, candidates: &[&str]) -> bool {
    candidates.iter().any(|candidate| value.contains(candidate))
}

fn role_name(role: MemoThreadMessageRole) -> &'static str {
    match role {
        MemoThreadMessageRole::User => "user",
        MemoThreadMessageRole::Assistant => "assistant",
        MemoThreadMessageRole::System => "system",
    }
}
