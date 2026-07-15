use std::collections::HashMap;

use serde_json::Value;
use sqlx::{Row, SqlitePool};

use crate::{
    ai::{ConversationSubjectCandidate, ConversationSubjectClarification},
    error::AppResult,
};

use super::{
    storage,
    types::{ThreadSubject, ThreadSubjectKind},
};

mod matching;

use matching::{
    contains_any, contains_symbol, extract_company_hint, is_derivative_or_fund,
    is_secondary_counter, is_strong_symbol_reference, looks_like_security_code, normalize_text,
    trim_company_name, valid_company_alias, valid_company_hint,
};

#[derive(Debug, Clone, PartialEq)]
pub enum SubjectResolution {
    Resolved(ThreadSubject),
    NeedsClarification(ConversationSubjectClarification),
}

pub async fn resolve_turn_subject(
    pool: &SqlitePool,
    thread_id: &str,
    user_message: &str,
) -> AppResult<(
    ThreadSubject,
    Option<ConversationSubjectClarification>,
    String,
)> {
    match resolve_subject(pool, thread_id, user_message).await? {
        SubjectResolution::Resolved(subject) => {
            let effective_message = resume_pending_request(pool, thread_id, user_message, &subject)
                .await?
                .unwrap_or_else(|| user_message.to_string());
            Ok((subject, None, effective_message))
        }
        SubjectResolution::NeedsClarification(clarification) => Ok((
            ThreadSubject::default(),
            Some(clarification),
            user_message.to_string(),
        )),
    }
}

#[derive(Debug)]
struct ScoredCandidate {
    company: ConversationSubjectCandidate,
    score: i32,
}

#[derive(Debug)]
struct PendingClarification {
    original_request: String,
    candidates: Vec<ConversationSubjectCandidate>,
}

pub async fn resolve_subject(
    pool: &SqlitePool,
    thread_id: &str,
    message: &str,
) -> AppResult<SubjectResolution> {
    let current = storage::thread_subject(pool, thread_id).await?;
    if let Some(pending) = load_pending_clarification(pool, thread_id).await? {
        if let Some(candidate) = select_pending_candidate(message, &pending.candidates) {
            return resolve_candidate(pool, thread_id, &current, candidate).await;
        }
        if !pending.candidates.is_empty() && is_pending_selection_reply(message) {
            return Ok(SubjectResolution::NeedsClarification(
                ConversationSubjectClarification {
                    target_hint: Some(message.trim().to_string()),
                    candidates: pending.candidates,
                },
            ));
        }
    }
    let target_hint = extract_company_hint(message);
    let candidates = company_candidates(pool, message, target_hint.as_deref(), &current).await?;

    if candidates.len() == 1 {
        return resolve_candidate(pool, thread_id, &current, &candidates[0]).await;
    }

    if candidates.len() > 1 || target_hint.is_some() {
        return Ok(SubjectResolution::NeedsClarification(
            ConversationSubjectClarification {
                target_hint,
                candidates,
            },
        ));
    }

    if current.kind_type() != ThreadSubjectKind::General {
        return Ok(SubjectResolution::Resolved(current));
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
    } else {
        current
    };
    if subject != ThreadSubject::default() {
        storage::update_thread_subject(pool, thread_id, subject.clone()).await?;
    }
    Ok(SubjectResolution::Resolved(subject))
}

async fn resume_pending_request(
    pool: &SqlitePool,
    thread_id: &str,
    confirmation: &str,
    subject: &ThreadSubject,
) -> AppResult<Option<String>> {
    if !is_pending_selection_reply(confirmation) {
        return Ok(None);
    }
    let Some(pending) = load_pending_clarification(pool, thread_id).await? else {
        return Ok(None);
    };
    let Some(symbol) = subject.subject_key.as_deref() else {
        return Ok(None);
    };
    if !pending.candidates.is_empty()
        && !pending
            .candidates
            .iter()
            .any(|candidate| candidate.symbol.eq_ignore_ascii_case(symbol))
    {
        return Ok(None);
    }
    let label = subject.label.as_deref().unwrap_or(symbol);
    Ok(Some(format!(
        "{}\n\n[Confirmed company: {label} ({symbol})]",
        pending.original_request
    )))
}

async fn resolve_candidate(
    pool: &SqlitePool,
    thread_id: &str,
    current: &ThreadSubject,
    candidate: &ConversationSubjectCandidate,
) -> AppResult<SubjectResolution> {
    let subject = ThreadSubject {
        kind: "company".to_string(),
        subject_key: Some(candidate.symbol.clone()),
        label: Some(candidate.name.clone()),
        confidence: 0.95,
    };
    if current.kind_type() == ThreadSubjectKind::General {
        storage::update_thread_subject(pool, thread_id, subject.clone()).await?;
    }
    Ok(SubjectResolution::Resolved(subject))
}

async fn load_pending_clarification(
    pool: &SqlitePool,
    thread_id: &str,
) -> AppResult<Option<PendingClarification>> {
    let raw = sqlx::query_scalar::<_, String>(
        r#"SELECT used_context_json FROM memo_thread_messages
        WHERE thread_id = ? AND role = 'assistant' AND status = 'completed'
        ORDER BY created_at DESC LIMIT 1"#,
    )
    .bind(thread_id)
    .fetch_optional(pool)
    .await?;
    let Some(entries) = raw
        .as_deref()
        .and_then(|value| serde_json::from_str::<Vec<Value>>(value).ok())
    else {
        return Ok(None);
    };
    let Some(entry) = entries
        .iter()
        .find(|entry| entry.get("kind").and_then(Value::as_str) == Some("subject_clarification"))
    else {
        return Ok(None);
    };
    let Some(original_request) = entry.get("original_request").and_then(Value::as_str) else {
        return Ok(None);
    };
    let candidates = entry
        .get("candidates")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default();
    Ok(Some(PendingClarification {
        original_request: original_request.to_string(),
        candidates,
    }))
}

fn select_pending_candidate<'a>(
    message: &str,
    candidates: &'a [ConversationSubjectCandidate],
) -> Option<&'a ConversationSubjectCandidate> {
    if candidates.is_empty() {
        return None;
    }
    if let Some(index) = ordinal_candidate_index(message) {
        return candidates.get(index);
    }
    let exact_symbols = candidates
        .iter()
        .filter(|candidate| candidate.symbol.eq_ignore_ascii_case(message.trim()))
        .collect::<Vec<_>>();
    if let [candidate] = exact_symbols.as_slice() {
        return Some(*candidate);
    }
    let mut scored = candidates
        .iter()
        .filter_map(|candidate| {
            let score = candidate_score(message, None, &candidate.symbol, &candidate.name, false);
            (score > 0).then_some((score, candidate))
        })
        .collect::<Vec<_>>();
    scored.sort_by(|left, right| right.0.cmp(&left.0));
    if scored.is_empty() || scored.get(1).is_some_and(|next| next.0 == scored[0].0) {
        None
    } else {
        Some(scored[0].1)
    }
}

fn ordinal_candidate_index(message: &str) -> Option<usize> {
    let normalized = normalize_text(message);
    let ordinals = [
        (0, ["第一个", "第1个", "选1", "前者"]),
        (1, ["第二个", "第2个", "选2", "后者"]),
        (2, ["第三个", "第3个", "选3", "第三者"]),
        (3, ["第四个", "第4个", "选4", "第四者"]),
        (4, ["第五个", "第5个", "选5", "第五者"]),
    ];
    for (index, aliases) in ordinals {
        if aliases.iter().any(|alias| normalized.contains(alias)) {
            return Some(index);
        }
    }
    normalized
        .trim_matches(|character: char| !character.is_ascii_digit())
        .parse::<usize>()
        .ok()
        .filter(|value| (1..=5).contains(value))
        .map(|value| value - 1)
}

fn is_pending_selection_reply(message: &str) -> bool {
    ordinal_candidate_index(message).is_some()
        || looks_like_security_code(message.trim())
        || valid_company_hint(message)
}

async fn company_candidates(
    pool: &SqlitePool,
    message: &str,
    target_hint: Option<&str>,
    current: &ThreadSubject,
) -> AppResult<Vec<ConversationSubjectCandidate>> {
    let mut scored = HashMap::<String, ScoredCandidate>::new();
    if current.kind_type() == ThreadSubjectKind::Company {
        if let (Some(symbol), Some(name)) = (&current.subject_key, &current.label) {
            add_candidate(&mut scored, message, target_hint, symbol, name, true);
        }
    }

    for row in sqlx::query("SELECT symbol, name FROM portfolio_positions")
        .fetch_all(pool)
        .await?
    {
        add_candidate(
            &mut scored,
            message,
            target_hint,
            row.try_get("symbol")?,
            row.try_get("name")?,
            true,
        );
    }

    let exact_rows = sqlx::query(
        r#"SELECT symbol, name FROM security_symbols
        WHERE length(symbol) >= 2 AND (
            lower(?) LIKE '%' || lower(name) || '%'
            OR upper(?) LIKE '%' || upper(symbol) || '%'
        ) ORDER BY length(name) LIMIT 80"#,
    )
    .bind(message)
    .bind(message)
    .fetch_all(pool)
    .await?;
    for row in exact_rows {
        add_candidate(
            &mut scored,
            message,
            target_hint,
            row.try_get("symbol")?,
            row.try_get("name")?,
            false,
        );
    }

    if let Some(hint) = target_hint {
        let hint_rows = sqlx::query(
            r#"SELECT symbol, name FROM security_symbols
            WHERE lower(name) LIKE '%' || lower(?) || '%'
               OR upper(symbol) = upper(?)
            ORDER BY length(name), symbol LIMIT 80"#,
        )
        .bind(hint)
        .bind(hint)
        .fetch_all(pool)
        .await?;
        for row in hint_rows {
            add_candidate(
                &mut scored,
                message,
                target_hint,
                row.try_get("symbol")?,
                row.try_get("name")?,
                false,
            );
        }
    }

    let Some(top_score) = scored.values().map(|candidate| candidate.score).max() else {
        return Ok(Vec::new());
    };
    if top_score < 140 {
        return Ok(Vec::new());
    }
    let minimum_score = if top_score >= 300 {
        top_score
    } else {
        (top_score - 50).max(140)
    };
    let mut candidates = scored
        .into_values()
        .filter(|candidate| candidate.score >= minimum_score)
        .map(|candidate| candidate.company)
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        left.name
            .chars()
            .count()
            .cmp(&right.name.chars().count())
            .then_with(|| left.symbol.cmp(&right.symbol))
    });
    candidates.truncate(5);
    Ok(candidates)
}

fn add_candidate(
    candidates: &mut HashMap<String, ScoredCandidate>,
    message: &str,
    target_hint: Option<&str>,
    symbol: &str,
    name: &str,
    preferred: bool,
) {
    let score = candidate_score(message, target_hint, symbol, name, preferred);
    if score == 0 {
        return;
    }
    let candidate = ScoredCandidate {
        company: ConversationSubjectCandidate {
            symbol: symbol.to_string(),
            name: name.to_string(),
        },
        score,
    };
    candidates
        .entry(symbol.to_ascii_uppercase())
        .and_modify(|existing| {
            if candidate.score > existing.score {
                existing.score = candidate.score;
                existing.company = candidate.company.clone();
            }
        })
        .or_insert(candidate);
}

fn candidate_score(
    message: &str,
    target_hint: Option<&str>,
    symbol: &str,
    name: &str,
    preferred: bool,
) -> i32 {
    let lower_message = normalize_text(message);
    let aliases = company_name_aliases(name);
    let mut score = if is_strong_symbol_reference(message, symbol, target_hint.is_some()) {
        400
    } else if aliases.iter().any(|alias| lower_message.contains(alias)) {
        300
    } else {
        0
    };

    if let Some(hint) = target_hint {
        let hint = normalize_text(hint);
        if symbol.eq_ignore_ascii_case(&hint) || contains_symbol(&hint, symbol) {
            score = score.max(400);
        } else if aliases.iter().any(|alias| alias == &hint) {
            score = score.max(300);
        } else if aliases.iter().any(|alias| alias.starts_with(&hint)) {
            score = score.max(220);
        } else if aliases.iter().any(|alias| alias.contains(&hint)) {
            score = score.max(180);
        }
    }

    if score == 0 {
        return 0;
    }
    if preferred {
        score += 20;
    }
    if is_secondary_counter(name) {
        score -= 60;
    }
    if is_derivative_or_fund(name) {
        score -= 100;
    }
    score
}

fn company_name_aliases(name: &str) -> Vec<String> {
    const SUFFIXES: &[&str] = &[
        "－ｗｒ",
        "-wr",
        "－ｓｗ",
        "-sw",
        "－ｗ",
        "-w",
        "－ｒ",
        "-r",
        "股份有限公司",
        "有限责任公司",
        "控股集团",
        "有限公司",
        " corporation",
        " incorporated",
        " holdings",
        " holding",
        " limited",
        " group",
        " corp.",
        " corp",
        " inc.",
        " inc",
        " ltd.",
        " ltd",
        "控股",
        "集团",
        "公司",
    ];
    let mut aliases = Vec::new();
    let mut candidate = trim_company_name(&normalize_text(name)).to_string();
    if valid_company_alias(&candidate) {
        aliases.push(candidate.clone());
    }
    loop {
        let Some(stripped) = SUFFIXES
            .iter()
            .find_map(|suffix| candidate.strip_suffix(suffix))
        else {
            break;
        };
        candidate = trim_company_name(stripped).to_string();
        if valid_company_alias(&candidate) {
            aliases.push(candidate.clone());
        }
    }
    aliases.sort();
    aliases.dedup();
    aliases
}

#[cfg(test)]
mod tests;
