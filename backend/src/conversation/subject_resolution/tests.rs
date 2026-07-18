use serde_json::json;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

use super::{
    extract_company_hint, resolve_subject, resolve_turn_subject, resume_pending_request,
    SubjectResolution,
};
use crate::{
    ai::ConversationSubjectCandidate,
    conversation::{
        research::plan_research,
        storage,
        types::{StartRunRequest, ThreadSubject},
    },
    database,
    locale::Locale,
};

#[tokio::test]
async fn held_company_short_name_resolves_without_rebinding_the_thread() {
    let (pool, thread_id) = bound_pdd_thread().await;
    insert_position(&pool, "0700.HK", "腾讯控股").await;

    let resolution = resolve_subject(&pool, &thread_id, "分析一下腾讯")
        .await
        .expect("resolve subject");
    let SubjectResolution::Resolved(subject) = resolution else {
        panic!("expected a resolved company");
    };
    let stored = storage::thread_subject(&pool, &thread_id)
        .await
        .expect("stored subject");

    assert_eq!(subject.subject_key.as_deref(), Some("0700.HK"));
    assert_eq!(stored.subject_key.as_deref(), Some("PDD"));
}

#[tokio::test]
async fn directory_short_name_selects_only_the_unique_primary_security() {
    let (pool, thread_id) = bound_pdd_thread().await;
    insert_security(&pool, "3690.HK", "美团－Ｗ").await;
    insert_security(&pool, "83690.HK", "美团－ＷＲ").await;
    insert_security(&pool, "13002.HK", "美团中银七一购A").await;

    let resolution = resolve_subject(&pool, &thread_id, "分析一下美团")
        .await
        .expect("resolve subject");
    let SubjectResolution::Resolved(subject) = resolution else {
        panic!("expected the primary security");
    };

    assert_eq!(subject.subject_key.as_deref(), Some("3690.HK"));
}

#[tokio::test]
async fn nasdaq_directory_description_does_not_hide_the_company_name() {
    let (pool, thread_id) = general_thread().await;
    insert_security(&pool, "NFLX", "Netflix, Inc. - Common Stock").await;
    let hint = extract_company_hint("分析 Netflix 的护城河");

    assert_eq!(hint.as_deref(), Some("netflix"));
    assert!(
        super::candidate_score(
            "分析 Netflix 的护城河",
            hint.as_deref(),
            "NFLX",
            "Netflix, Inc. - Common Stock",
            false,
        ) >= 140
    );

    let resolution = resolve_subject(&pool, &thread_id, "分析 Netflix 的护城河")
        .await
        .expect("resolve subject");
    let SubjectResolution::Resolved(subject) = resolution else {
        panic!("expected Netflix to resolve from the Nasdaq directory name");
    };

    assert_eq!(subject.subject_key.as_deref(), Some("NFLX"));
}

#[tokio::test]
async fn unique_transposition_typo_resolves_without_confirmation() {
    let (pool, thread_id) = general_thread().await;
    insert_security(&pool, "NFLX", "Netflix, Inc. - Common Stock").await;

    let resolution = resolve_subject(&pool, &thread_id, "分析 netlfix 的护城河")
        .await
        .expect("resolve subject");
    let SubjectResolution::Resolved(subject) = resolution else {
        panic!("expected the unique high-confidence fuzzy company match");
    };

    assert_eq!(subject.subject_key.as_deref(), Some("NFLX"));
}

#[tokio::test]
async fn tied_fuzzy_company_matches_still_require_confirmation() {
    let (pool, thread_id) = general_thread().await;
    insert_security(&pool, "ABCD", "Netlix Holdings").await;
    insert_security(&pool, "NFLX", "Netflix, Inc. - Common Stock").await;

    let resolution = resolve_subject(&pool, &thread_id, "分析 netlfix 的护城河")
        .await
        .expect("resolve subject");
    let SubjectResolution::NeedsClarification(clarification) = resolution else {
        panic!("expected ambiguous fuzzy matches to require confirmation");
    };

    assert_eq!(clarification.candidates.len(), 2);
}

#[tokio::test]
async fn ambiguous_short_name_requires_confirmation_instead_of_using_bound_company() {
    let (pool, thread_id) = bound_pdd_thread().await;
    insert_security(&pool, "9988.HK", "阿里巴巴－Ｗ").await;
    insert_security(&pool, "0241.HK", "阿里健康").await;

    let resolution = resolve_subject(&pool, &thread_id, "分析一下阿里")
        .await
        .expect("resolve subject");
    let SubjectResolution::NeedsClarification(clarification) = resolution else {
        panic!("expected clarification");
    };

    assert_eq!(clarification.target_hint.as_deref(), Some("阿里"));
    assert_eq!(clarification.candidates.len(), 2);
    assert!(clarification
        .candidates
        .iter()
        .any(|candidate| candidate.name == "阿里巴巴－Ｗ"));
    assert!(clarification
        .candidates
        .iter()
        .any(|candidate| candidate.name == "阿里健康"));
}

#[tokio::test]
async fn fuzzy_short_name_keeps_close_candidates_instead_of_guessing_the_top_one() {
    let (pool, thread_id) = bound_pdd_thread().await;
    insert_security(&pool, "000001.SZ", "平安银行").await;
    insert_security(&pool, "601318.SS", "中国平安").await;

    let resolution = resolve_subject(&pool, &thread_id, "分析一下平安")
        .await
        .expect("resolve subject");
    let SubjectResolution::NeedsClarification(clarification) = resolution else {
        panic!("expected clarification");
    };

    assert_eq!(clarification.candidates.len(), 2);
    assert!(clarification
        .candidates
        .iter()
        .any(|candidate| candidate.name == "平安银行"));
    assert!(clarification
        .candidates
        .iter()
        .any(|candidate| candidate.name == "中国平安"));
}

#[tokio::test]
async fn unknown_explicit_company_requires_confirmation_instead_of_falling_back() {
    let (pool, thread_id) = bound_pdd_thread().await;

    let resolution = resolve_subject(&pool, &thread_id, "分析一下某某科技")
        .await
        .expect("resolve subject");
    let SubjectResolution::NeedsClarification(clarification) = resolution else {
        panic!("expected clarification");
    };

    assert_eq!(clarification.target_hint.as_deref(), Some("某某科技"));
    assert!(clarification.candidates.is_empty());
}

#[tokio::test]
async fn pronoun_follow_up_keeps_the_bound_company() {
    let (pool, thread_id) = bound_pdd_thread().await;

    let resolution = resolve_subject(&pool, &thread_id, "继续分析它的商业模式")
        .await
        .expect("resolve subject");
    let SubjectResolution::Resolved(subject) = resolution else {
        panic!("expected bound subject");
    };

    assert_eq!(subject.subject_key.as_deref(), Some("PDD"));
}

#[tokio::test]
async fn generic_company_view_update_keeps_the_bound_company() {
    let (pool, thread_id) = bound_pdd_thread().await;

    let resolution = resolve_subject(
        &pool,
        &thread_id,
        "请把上一轮结论提议为公司看法更新，补充护城河和风险。",
    )
    .await
    .expect("resolve subject");
    let SubjectResolution::Resolved(subject) = resolution else {
        panic!("expected bound subject");
    };

    assert_eq!(subject.subject_key.as_deref(), Some("PDD"));
}

#[tokio::test]
async fn lowercase_social_word_does_not_become_a_short_us_ticker() {
    let (pool, thread_id) = general_thread().await;
    insert_security(&pool, "HI", "Hillenbrand Inc").await;

    let resolution = resolve_subject(&pool, &thread_id, "hi")
        .await
        .expect("resolve subject");
    let SubjectResolution::Resolved(subject) = resolution else {
        panic!("expected general subject");
    };

    assert_eq!(subject, ThreadSubject::default());
}

#[tokio::test]
async fn lowercase_ticker_in_an_explicit_company_request_resolves_and_binds() {
    let (pool, thread_id) = general_thread().await;
    insert_position(&pool, "PDD", "拼多多").await;

    let resolution = resolve_subject(
        &pool,
        &thread_id,
        "我想投资pdd，可以给我介绍一下这个公司吗？",
    )
    .await
    .expect("resolve subject");
    let SubjectResolution::Resolved(subject) = resolution else {
        panic!("expected a resolved company");
    };
    let stored = storage::thread_subject(&pool, &thread_id)
        .await
        .expect("stored subject");
    let follow_up = resolve_subject(&pool, &thread_id, "生成详细的护城河分析")
        .await
        .expect("resolve follow-up");
    let SubjectResolution::Resolved(follow_up) = follow_up else {
        panic!("expected follow-up to keep the company");
    };

    assert_eq!(subject.subject_key.as_deref(), Some("PDD"));
    assert_eq!(stored.subject_key.as_deref(), Some("PDD"));
    assert_eq!(follow_up.subject_key.as_deref(), Some("PDD"));
}

#[tokio::test]
async fn ordinal_confirmation_resumes_the_original_request() {
    let (pool, thread_id) = bound_pdd_thread().await;
    insert_pending_clarification(&pool, &thread_id).await;

    let resolution = resolve_subject(&pool, &thread_id, "第一个")
        .await
        .expect("resolve confirmation");
    let SubjectResolution::Resolved(subject) = resolution else {
        panic!("expected confirmed company");
    };
    let resumed = resume_pending_request(&pool, &thread_id, "第一个", &subject)
        .await
        .expect("resume request")
        .expect("pending request");

    assert_eq!(subject.subject_key.as_deref(), Some("9988.HK"));
    assert!(resumed.starts_with("分析一下阿里"));
    assert!(resumed.contains("阿里巴巴－Ｗ (9988.HK)"));
}

#[tokio::test]
async fn affirmative_single_candidate_confirmation_resumes_research_request() {
    let (pool, thread_id) = general_thread().await;
    insert_single_candidate_pending_clarification(&pool, &thread_id).await;

    let (subject, clarification, effective_message) =
        resolve_turn_subject(&pool, &thread_id, "是的")
            .await
            .expect("resolve confirmation");

    assert!(clarification.is_none());
    assert_eq!(subject.subject_key.as_deref(), Some("NFLX"));
    assert!(effective_message.starts_with("分析网飞的护城河"));
    assert!(effective_message.contains("Netflix (NFLX)"));
    assert!(plan_research(&effective_message, &subject).is_some());
}

#[tokio::test]
async fn affirmative_reply_recovers_a_natural_single_company_clarification() {
    let (pool, thread_id) = general_thread().await;
    insert_security(&pool, "NFLX", "Netflix").await;
    insert_empty_pending_clarification(&pool, &thread_id).await;
    insert_natural_company_clarification(&pool, &thread_id).await;

    let (subject, clarification, effective_message) =
        resolve_turn_subject(&pool, &thread_id, "是的")
            .await
            .expect("resolve confirmation");

    assert!(clarification.is_none());
    assert_eq!(subject.subject_key.as_deref(), Some("NFLX"));
    assert!(effective_message.starts_with("分析网飞的护城河"));
    assert!(plan_research(&effective_message, &subject).is_some());
}

#[tokio::test]
async fn symbol_confirmation_selects_only_the_matching_candidate() {
    let (pool, thread_id) = bound_pdd_thread().await;
    insert_pending_clarification(&pool, &thread_id).await;

    let resolution = resolve_subject(&pool, &thread_id, "0241.HK")
        .await
        .expect("resolve confirmation");
    let SubjectResolution::Resolved(subject) = resolution else {
        panic!("expected confirmed company");
    };

    assert_eq!(subject.subject_key.as_deref(), Some("0241.HK"));
}

#[tokio::test]
async fn unrecognized_confirmation_stays_in_clarification_instead_of_using_bound_company() {
    let (pool, thread_id) = bound_pdd_thread().await;
    insert_pending_clarification(&pool, &thread_id).await;

    let resolution = resolve_subject(&pool, &thread_id, "1234.HK")
        .await
        .expect("resolve confirmation");
    let SubjectResolution::NeedsClarification(clarification) = resolution else {
        panic!("expected another clarification");
    };

    assert_eq!(clarification.target_hint.as_deref(), Some("1234.HK"));
    assert_eq!(clarification.candidates.len(), 2);
}

#[tokio::test]
async fn new_request_supersedes_a_pending_company_clarification() {
    let (pool, thread_id) = bound_pdd_thread().await;
    insert_pending_clarification(&pool, &thread_id).await;

    let resolution = resolve_subject(
        &pool,
        &thread_id,
        "请将上一轮已经形成的明确结论生成公司看法更新提议，不增加新事实。",
    )
    .await
    .expect("resolve new request");
    let SubjectResolution::Resolved(subject) = resolution else {
        panic!("expected the new request to use the bound company");
    };

    assert_eq!(subject.subject_key.as_deref(), Some("PDD"));
}

#[tokio::test]
async fn unknown_standalone_security_code_never_falls_back_to_bound_company() {
    let (pool, thread_id) = bound_pdd_thread().await;

    let resolution = resolve_subject(&pool, &thread_id, "1234.HK")
        .await
        .expect("resolve subject");
    let SubjectResolution::NeedsClarification(clarification) = resolution else {
        panic!("expected clarification");
    };

    assert_eq!(clarification.target_hint.as_deref(), Some("1234.HK"));
    assert!(clarification.candidates.is_empty());
}

#[test]
fn extracts_explicit_company_hints_without_treating_pronouns_as_companies() {
    assert_eq!(
        extract_company_hint("分析一下腾讯").as_deref(),
        Some("腾讯")
    );
    assert_eq!(extract_company_hint("美团怎么样").as_deref(), Some("美团"));
    assert_eq!(
        extract_company_hint("请问腾讯的护城河").as_deref(),
        Some("腾讯")
    );
    assert_eq!(
        extract_company_hint("分析一下 PDD 最新财报").as_deref(),
        Some("pdd")
    );
    assert_eq!(
        extract_company_hint("研究拼多多近五年财报情况，分析护城河").as_deref(),
        Some("拼多多")
    );
    assert_eq!(extract_company_hint("继续分析它的商业模式"), None);
    assert_eq!(extract_company_hint("分析一下我的持仓"), None);
    assert_eq!(
        extract_company_hint("你好，这是临时公司备忘录闭环验收线程。"),
        None
    );
    assert_eq!(extract_company_hint("请帮我复盘今天的投资判断。"), None);
    assert_eq!(
        extract_company_hint("请更新公司看法，补充护城河和风险。"),
        None
    );
}

async fn bound_pdd_thread() -> (SqlitePool, String) {
    let (pool, thread_id) = general_thread().await;
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
    (pool, thread_id)
}

async fn general_thread() -> (SqlitePool, String) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("connect sqlite");
    database::migrate(&pool).await.expect("migrate");
    let request = StartRunRequest {
        client_request_id: "subject-resolution-test".to_string(),
        thread_id: None,
        client_thread_id: Some("subject-resolution-thread".to_string()),
        content: "分析拼多多".to_string(),
        attachment_ids: Vec::new(),
        locale: Some("zh-CN".to_string()),
    };
    let (_, thread_id) = storage::create_run(&pool, &request, Locale::Zh, None)
        .await
        .expect("create run");
    (pool, thread_id)
}

async fn insert_pending_clarification(pool: &SqlitePool, thread_id: &str) {
    let candidates = vec![
        ConversationSubjectCandidate {
            symbol: "9988.HK".to_string(),
            name: "阿里巴巴－Ｗ".to_string(),
        },
        ConversationSubjectCandidate {
            symbol: "0241.HK".to_string(),
            name: "阿里健康".to_string(),
        },
    ];
    let used_context = json!([{
        "kind": "subject_clarification",
        "original_request": "分析一下阿里",
        "target_hint": "阿里",
        "candidates": candidates,
    }]);
    sqlx::query(
        r#"INSERT INTO memo_thread_messages (
            id, thread_id, role, content, status, request_id, artifacts_json,
            sources_json, used_context_json, created_at, updated_at
        ) VALUES (
            'pending-clarification', ?, 'assistant', '请确认公司', 'completed',
            'pending-request', '[]', '[]', ?,
            '2026-01-02T00:00:00Z', '2026-01-02T00:00:00Z'
        )"#,
    )
    .bind(thread_id)
    .bind(used_context.to_string())
    .execute(pool)
    .await
    .expect("insert pending clarification");
}

async fn insert_single_candidate_pending_clarification(pool: &SqlitePool, thread_id: &str) {
    let candidates = vec![ConversationSubjectCandidate {
        symbol: "NFLX".to_string(),
        name: "Netflix".to_string(),
    }];
    let used_context = json!([{
        "kind": "subject_clarification",
        "original_request": "分析网飞的护城河",
        "target_hint": "网飞",
        "candidates": candidates,
    }]);
    sqlx::query(
        r#"INSERT INTO memo_thread_messages (
            id, thread_id, role, content, status, request_id, artifacts_json,
            sources_json, used_context_json, created_at, updated_at
        ) VALUES (
            'single-pending-clarification', ?, 'assistant', '你指的是 Netflix 吗？', 'completed',
            'single-pending-request', '[]', '[]', ?,
            '2026-01-02T00:00:00Z', '2026-01-02T00:00:00Z'
        )"#,
    )
    .bind(thread_id)
    .bind(used_context.to_string())
    .execute(pool)
    .await
    .expect("insert pending clarification");
}

async fn insert_empty_pending_clarification(pool: &SqlitePool, thread_id: &str) {
    let used_context = json!([{
        "kind": "subject_clarification",
        "original_request": "分析网飞的护城河",
        "target_hint": "网飞",
        "candidates": [],
    }]);
    sqlx::query(
        r#"INSERT INTO memo_thread_messages (
            id, thread_id, role, content, status, request_id, artifacts_json,
            sources_json, used_context_json, created_at, updated_at
        ) VALUES (
            'empty-pending-clarification', ?, 'assistant', '请提供公司全名或证券代码。', 'completed',
            'empty-pending-request', '[]', '[]', ?,
            '2026-01-02T00:00:00Z', '2026-01-02T00:00:00Z'
        )"#,
    )
    .bind(thread_id)
    .bind(used_context.to_string())
    .execute(pool)
    .await
    .expect("insert empty pending clarification");
}

async fn insert_natural_company_clarification(pool: &SqlitePool, thread_id: &str) {
    sqlx::query(
        r#"INSERT INTO memo_thread_messages (
            id, thread_id, role, content, status, request_id, artifacts_json,
            sources_json, used_context_json, created_at, updated_at
        ) VALUES (
            'natural-company-clarification', ?, 'assistant',
            '你指的是 Netflix（奈飞，NASDAQ：NFLX）吗？', 'canceled',
            'natural-company-request', '[]', '[]', '[]',
            '2026-01-02T00:01:00Z', '2026-01-02T00:01:00Z'
        )"#,
    )
    .bind(thread_id)
    .execute(pool)
    .await
    .expect("insert natural company clarification");
}

async fn insert_position(pool: &SqlitePool, symbol: &str, name: &str) {
    sqlx::query(
        r#"INSERT INTO portfolio_positions (
            symbol, name, asset_type, quantity, average_cost, currency, market,
            market_value, unrealized_pnl, weight, price_stale, updated_at
        ) VALUES (?, ?, 'stock', 1, 1, 'HKD', 'HK', 1, 0, 1, 0,
                  '2026-01-01T00:00:00Z')"#,
    )
    .bind(symbol)
    .bind(name)
    .execute(pool)
    .await
    .expect("insert position");
}

async fn insert_security(pool: &SqlitePool, symbol: &str, name: &str) {
    sqlx::query(
        "INSERT INTO security_symbols (symbol, name, market, currency, updated_at) VALUES (?, ?, 'HK', 'HKD', '2026-01-01T00:00:00Z')",
    )
    .bind(symbol)
    .bind(name)
    .execute(pool)
    .await
    .expect("insert security");
}
