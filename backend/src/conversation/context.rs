use std::{collections::HashMap, path::Path};

use serde_json::json;
use sqlx::SqlitePool;

use crate::{
    ai::{
        ConversationContext, ConversationResearchSource, ConversationSubjectClarification,
        MemoChatHistoryMessage,
    },
    error::AppResult,
    investment_system::{active_rule_graph, legacy_system},
    memo_thread::{self, MemoThreadMessageRole},
    portfolio,
};

use super::{
    attachments::load_attachment_contexts,
    company::load_company_view,
    storage,
    types::{ThreadSubject, ThreadSubjectKind},
};

const MAX_RESEARCH_SOURCES_PER_TIER: usize = 3;
const MAX_PRIMARY_RESEARCH_SNIPPET_CHARS: usize = 8_000;
const MAX_SUPPORTING_RESEARCH_SNIPPET_CHARS: usize = 2_500;

pub struct ConversationResearchContext {
    pub sources: Vec<ConversationResearchSource>,
    pub warning: Option<String>,
}

pub async fn assemble_subject_clarification_context(
    pool: &SqlitePool,
    user_message: &str,
    clarification: &ConversationSubjectClarification,
) -> AppResult<ConversationContext> {
    let label = if clarification.candidates.is_empty() {
        clarification
            .target_hint
            .clone()
            .unwrap_or_else(|| "company".to_string())
    } else {
        clarification
            .candidates
            .iter()
            .map(|candidate| format!("{} ({})", candidate.name, candidate.symbol))
            .collect::<Vec<_>>()
            .join(" / ")
    };
    Ok(ConversationContext {
        thread_title: String::new(),
        thread_summary: String::new(),
        turn_summaries: Vec::new(),
        subject: serde_json::to_value(ThreadSubject::default())?,
        user_message: user_message.to_string(),
        recent_messages: Vec::new(),
        portfolio_summary: portfolio::summary(pool).await?,
        portfolio_positions: Vec::new(),
        company_view: None,
        recent_trades: Vec::new(),
        investment_system: json!({}),
        attachments: Vec::new(),
        research_sources: Vec::new(),
        research_warning: None,
        capability_artifacts: Vec::new(),
        subject_clarification: Some(clarification.clone()),
        used_context: vec![json!({
            "kind": "subject_clarification",
            "label": label,
            "original_request": user_message,
            "target_hint": &clarification.target_hint,
            "candidates": &clarification.candidates,
        })],
    })
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
        sources,
        warning: research_warning,
    } = research;
    let research_sources = compact_research_sources(sources);
    let bound_subject = storage::thread_subject(pool, thread_id).await?;
    let alternate_company_turn = is_alternate_company_turn(&bound_subject, subject);
    let detail = memo_thread::get_detail(pool, thread_id, 10, None).await?;
    let portfolio_positions =
        positions_for_subject(portfolio::list_positions(pool).await?, subject);
    let portfolio_summary = portfolio::summary(pool).await?;
    let company_view = match subject
        .subject_key
        .as_deref()
        .filter(|_| subject.kind_type() == ThreadSubjectKind::Company)
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
        .filter(|_| subject.kind_type() == ThreadSubjectKind::Company)
    {
        Some(symbol) => portfolio::recent_trade_events(pool, symbol, 8)
            .await?
            .into_iter()
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()?,
        None => Vec::new(),
    };
    let active_graph = active_rule_graph(pool).await?;
    let legacy = legacy_system(pool).await?;
    let attachments =
        compact_attachments(load_attachment_contexts(pool, workspace_dir, run_id).await?);
    let mut turn_summaries = sqlx::query_scalar::<_, String>(
        r#"SELECT summary FROM conversation_turn_summaries
        WHERE thread_id = ? AND subject_kind = ?
          AND COALESCE(subject_key, '') = COALESCE(?, '')
        ORDER BY created_at DESC LIMIT 4"#,
    )
    .bind(thread_id)
    .bind(&subject.kind)
    .bind(&subject.subject_key)
    .fetch_all(pool)
    .await?;
    turn_summaries.reverse();
    let recent_messages = if alternate_company_turn {
        Vec::new()
    } else {
        detail
            .messages
            .into_iter()
            .filter(|message| {
                !(message.role == MemoThreadMessageRole::Assistant && message.content.is_empty())
            })
            .map(|message| MemoChatHistoryMessage {
                role: role_name(message.role).to_string(),
                content: message.content,
            })
            .collect::<Vec<_>>()
    };
    let mut used_context = Vec::new();
    if !alternate_company_turn {
        used_context.push(json!({ "kind": "thread_summary", "label": detail.thread.title }));
    }
    if !turn_summaries.is_empty() {
        used_context.push(
            json!({ "kind": "turn_summaries", "label": format!("{} prior turns", turn_summaries.len()) }),
        );
    }
    if include_cross_subject_context(subject) {
        used_context.push(
            json!({ "kind": "portfolio", "label": format!("{} positions", portfolio_summary.positions_count) }),
        );
        used_context.push(
            json!({ "kind": "investment_system", "label": format!("rule graph v{}", active_graph.version) }),
        );
    }
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

    let thread_title = if alternate_company_turn {
        subject
            .label
            .clone()
            .or_else(|| subject.subject_key.clone())
            .unwrap_or_default()
    } else {
        detail.thread.title
    };
    let thread_summary = if alternate_company_turn {
        String::new()
    } else {
        detail.thread.summary
    };
    Ok(ConversationContext {
        thread_title,
        thread_summary,
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
        capability_artifacts: Vec::new(),
        subject_clarification: None,
        used_context,
    })
}

fn is_alternate_company_turn(bound: &ThreadSubject, effective: &ThreadSubject) -> bool {
    if bound.kind_type() != ThreadSubjectKind::Company
        || effective.kind_type() != ThreadSubjectKind::Company
    {
        return false;
    }
    match (
        bound.subject_key.as_deref(),
        effective.subject_key.as_deref(),
    ) {
        (Some(bound_symbol), Some(effective_symbol)) => {
            !bound_symbol.eq_ignore_ascii_case(effective_symbol)
        }
        _ => false,
    }
}

fn include_cross_subject_context(subject: &ThreadSubject) -> bool {
    subject.kind_type() != ThreadSubjectKind::Company
}

fn role_name(role: MemoThreadMessageRole) -> &'static str {
    match role {
        MemoThreadMessageRole::User => "user",
        MemoThreadMessageRole::Assistant => "assistant",
        MemoThreadMessageRole::System => "system",
    }
}

fn compact_research_sources(
    sources: Vec<ConversationResearchSource>,
) -> Vec<ConversationResearchSource> {
    let mut counts = HashMap::<String, usize>::new();
    sources
        .into_iter()
        .filter_map(|mut source| {
            let count = counts.entry(source.source_tier.clone()).or_default();
            if *count >= MAX_RESEARCH_SOURCES_PER_TIER {
                return None;
            }
            *count += 1;
            let limit = if source.source_tier == "primary" {
                MAX_PRIMARY_RESEARCH_SNIPPET_CHARS
            } else {
                MAX_SUPPORTING_RESEARCH_SNIPPET_CHARS
            };
            source.snippet = truncate_chars(&source.snippet, limit);
            Some(source)
        })
        .collect()
}

fn positions_for_subject(
    positions: Vec<portfolio::PortfolioPosition>,
    subject: &ThreadSubject,
) -> Vec<portfolio::PortfolioPosition> {
    let Some(symbol) = subject
        .subject_key
        .as_deref()
        .filter(|_| subject.kind_type() == ThreadSubjectKind::Company)
    else {
        return positions;
    };
    positions
        .into_iter()
        .filter(|position| position.symbol.eq_ignore_ascii_case(symbol))
        .collect()
}

fn compact_attachments(
    attachments: Vec<crate::ai::ConversationAttachmentContext>,
) -> Vec<crate::ai::ConversationAttachmentContext> {
    attachments
        .into_iter()
        .take(4)
        .map(|mut attachment| {
            attachment.extracted_text = attachment
                .extracted_text
                .map(|text| truncate_chars(&text, 4_000));
            attachment
        })
        .collect()
}

fn truncate_chars(value: &str, limit: usize) -> String {
    let mut result = value.chars().take(limit).collect::<String>();
    if value.chars().count() > limit {
        result.push_str("...");
    }
    result
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

    use super::{
        assemble_context, compact_research_sources, include_cross_subject_context,
        ConversationResearchContext,
    };
    use crate::ai::ConversationResearchSource;
    use crate::{
        conversation::{
            storage,
            types::{StartRunRequest, ThreadSubject},
        },
        database,
        locale::Locale,
    };

    #[tokio::test]
    async fn alternate_company_turn_excludes_bound_company_history() {
        let (pool, run_id, thread_id) = bound_pdd_thread_with_tencent_position().await;
        let pdd = ThreadSubject {
            kind: "company".to_string(),
            subject_key: Some("PDD".to_string()),
            label: Some("拼多多".to_string()),
            confidence: 0.95,
        };
        storage::insert_turn_summary(&pool, &run_id, &thread_id, &pdd, "PDD summary")
            .await
            .expect("insert PDD summary");
        let tencent = ThreadSubject {
            kind: "company".to_string(),
            subject_key: Some("0700.HK".to_string()),
            label: Some("腾讯控股".to_string()),
            confidence: 0.95,
        };

        let context = assemble_context(
            &pool,
            Path::new("."),
            &run_id,
            &thread_id,
            "分析一下腾讯",
            &tencent,
            ConversationResearchContext {
                sources: Vec::new(),
                warning: None,
            },
        )
        .await
        .expect("assemble alternate-company context");

        assert_eq!(context.thread_title, "腾讯控股");
        assert!(context.thread_summary.is_empty());
        assert!(context.turn_summaries.is_empty());
        assert!(context.recent_messages.is_empty());
        assert!(context.used_context.iter().all(|item| {
            !matches!(
                item.get("kind").and_then(serde_json::Value::as_str),
                Some("thread_summary" | "turn_summaries")
            )
        }));
        assert!(context.used_context.iter().any(|item| {
            item.get("kind").and_then(serde_json::Value::as_str) == Some("company")
                && item.get("label").and_then(serde_json::Value::as_str) == Some("0700.HK")
        }));
    }

    #[tokio::test]
    async fn alternate_company_summary_does_not_replace_the_bound_thread_summary() {
        let (pool, run_id, thread_id) = bound_pdd_thread_with_tencent_position().await;
        let tencent = ThreadSubject {
            kind: "company".to_string(),
            subject_key: Some("0700.HK".to_string()),
            label: Some("腾讯控股".to_string()),
            confidence: 0.95,
        };

        storage::insert_turn_summary(&pool, &run_id, &thread_id, &tencent, "Tencent summary")
            .await
            .expect("insert Tencent summary");
        let thread_summary =
            sqlx::query_scalar::<_, String>("SELECT summary FROM memo_threads WHERE id = ?")
                .bind(&thread_id)
                .fetch_one(&pool)
                .await
                .expect("load thread summary");
        let summary_subject = sqlx::query_as::<_, (String, Option<String>)>(
            "SELECT subject_kind, subject_key FROM conversation_turn_summaries WHERE run_id = ?",
        )
        .bind(&run_id)
        .fetch_one(&pool)
        .await
        .expect("load summary subject");

        assert!(thread_summary.is_empty());
        assert_eq!(
            summary_subject,
            ("company".to_string(), Some("0700.HK".to_string()))
        );
    }

    async fn bound_pdd_thread_with_tencent_position() -> (SqlitePool, String, String) {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect sqlite");
        database::migrate(&pool).await.expect("migrate");
        let request = StartRunRequest {
            client_request_id: "cross-company-subject-test".to_string(),
            thread_id: None,
            client_thread_id: Some("cross-company-thread".to_string()),
            content: "分析拼多多".to_string(),
            attachment_ids: Vec::new(),
            locale: Some("zh-CN".to_string()),
        };
        let (run, thread_id) = storage::create_run(&pool, &request, Locale::Zh, None)
            .await
            .expect("create run");
        storage::update_thread_subject(
            &pool,
            &thread_id,
            ThreadSubject {
                kind: "company".to_string(),
                subject_key: Some("PDD".to_string()),
                label: Some("拼多多".to_string()),
                confidence: 0.95,
            },
        )
        .await
        .expect("bind PDD thread");
        sqlx::query(
            r#"INSERT INTO portfolio_positions (
                symbol, name, asset_type, quantity, average_cost, currency, market,
                market_value, unrealized_pnl, weight, price_stale, updated_at
            ) VALUES ('0700.HK', '腾讯控股', 'stock', 1, 1, 'HKD', 'HK', 1, 0, 1, 0,
                      '2026-01-01T00:00:00Z')"#,
        )
        .execute(&pool)
        .await
        .expect("insert Tencent position");
        (pool, run.id, thread_id)
    }

    #[test]
    fn company_threads_exclude_portfolio_and_rule_graph_context_labels() {
        let company = ThreadSubject {
            kind: "company".to_string(),
            subject_key: Some("PDD".to_string()),
            label: Some("PDD Holdings".to_string()),
            confidence: 0.95,
        };

        assert!(!include_cross_subject_context(&company));
        assert!(include_cross_subject_context(&ThreadSubject::default()));
    }

    #[test]
    fn conversation_research_context_is_bounded_by_tier_and_excerpt_size() {
        let mut sources = Vec::new();
        for tier in ["primary", "secondary", "community"] {
            for index in 0..4 {
                sources.push(ConversationResearchSource {
                    title: format!("{tier}-{index}"),
                    url: format!("https://example.com/{tier}/{index}"),
                    snippet: "evidence ".repeat(1_200),
                    source_tier: tier.to_string(),
                });
            }
        }

        let compact = compact_research_sources(sources);

        assert_eq!(compact.len(), 9);
        for tier in ["primary", "secondary", "community"] {
            assert_eq!(
                compact
                    .iter()
                    .filter(|source| source.source_tier == tier)
                    .count(),
                3
            );
        }
        assert!(compact
            .iter()
            .filter(|source| source.source_tier == "primary")
            .all(|source| source.snippet.chars().count() <= 8_003));
        assert!(compact
            .iter()
            .filter(|source| source.source_tier != "primary")
            .all(|source| source.snippet.chars().count() <= 2_503));
    }
}
