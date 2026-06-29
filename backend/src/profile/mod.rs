use axum::{extract::State, http::HeaderMap, Json};
use serde::Serialize;
use sqlx::{Row, SqlitePool};

use crate::{error::AppResult, locale::Locale, portfolio, state::AppState};

#[derive(Debug, Clone, Serialize)]
pub struct InvestorProfile {
    pub level: u32,
    pub xp: u32,
    pub next_level_xp: u32,
    pub attributes: Vec<ProfileAttribute>,
    pub badges: Vec<Badge>,
    pub bias_signals: Vec<String>,
    pub rule_events: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProfileAttribute {
    pub name: String,
    pub score: u32,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Badge {
    pub name: String,
    pub description: String,
}

pub async fn get_profile(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> AppResult<Json<InvestorProfile>> {
    Ok(Json(
        calculate_with_locale(&state.pool, Locale::from_headers(&headers)).await?,
    ))
}

pub async fn calculate(pool: &SqlitePool) -> AppResult<InvestorProfile> {
    calculate_with_locale(pool, Locale::En).await
}

pub async fn calculate_with_locale(
    pool: &SqlitePool,
    locale: Locale,
) -> AppResult<InvestorProfile> {
    let memo_count = count(pool, "SELECT COUNT(*) AS count FROM memos").await?;
    let memo_with_risk_count = count(
        pool,
        "SELECT COUNT(*) AS count FROM memos WHERE length(risks) > 0 OR length(disconfirming_evidence) > 0",
    )
    .await?;
    let decision_count = count(pool, "SELECT COUNT(*) AS count FROM decisions").await?;
    let reviewed_decision_count = count(
        pool,
        "SELECT COUNT(*) AS count FROM decisions WHERE review_date IS NOT NULL AND length(review_date) > 0",
    )
    .await?;
    let positions = portfolio::list_positions(pool).await?;

    let xp = (memo_count * 25)
        + (memo_with_risk_count * 20)
        + (decision_count * 35)
        + (reviewed_decision_count * 25)
        + ((positions.len() as u32) * 8);
    let level = (xp / 100) + 1;
    let next_level_xp = level * 100;

    let attributes = vec![
        ProfileAttribute {
            name: text(locale, "Thesis Builder", "Thesis 建筑师"),
            score: bounded_score(memo_count * 12 + memo_with_risk_count * 8),
            description: text(
                locale,
                "Turns raw ideas into explicit investable memos.",
                "把原始想法变成明确可复盘的投资备忘录。",
            ),
        },
        ProfileAttribute {
            name: text(locale, "Risk Scout", "风险侦察兵"),
            score: bounded_score(memo_with_risk_count * 18),
            description: text(
                locale,
                "Names risks and disconfirming evidence before commitment.",
                "在投入前命名风险和反证证据。",
            ),
        },
        ProfileAttribute {
            name: text(locale, "Decision Discipline", "决策纪律"),
            score: bounded_score(decision_count * 10 + reviewed_decision_count * 15),
            description: text(
                locale,
                "Records decisions with rationale, confidence, and review dates.",
                "记录决策理由、信心程度和复盘日期。",
            ),
        },
        ProfileAttribute {
            name: text(locale, "Portfolio Steward", "组合守护者"),
            score: bounded_score((positions.len() as u32) * 14),
            description: text(
                locale,
                "Keeps holdings visible, weighted, and ready for review.",
                "保持持仓可见、可衡量、可复盘。",
            ),
        },
    ];

    let mut badges = Vec::new();
    if memo_count > 0 {
        badges.push(Badge {
            name: text(locale, "First Memo", "第一份备忘录"),
            description: text(
                locale,
                "Logged the first investment memo.",
                "记录了第一份投资备忘录。",
            ),
        });
    }
    if memo_with_risk_count >= 3 {
        badges.push(Badge {
            name: text(locale, "Risk-First Thinker", "风险优先思考者"),
            description: text(
                locale,
                "Repeatedly wrote risks or disconfirming evidence.",
                "多次记录风险或反证证据。",
            ),
        });
    }
    if positions.len() >= 5 {
        badges.push(Badge {
            name: text(locale, "Portfolio Cartographer", "组合地图师"),
            description: text(
                locale,
                "Mapped at least five holdings into the workspace.",
                "已将至少五个持仓映射到工作台。",
            ),
        });
    }
    if reviewed_decision_count > 0 {
        badges.push(Badge {
            name: text(locale, "Review Loop", "复盘闭环"),
            description: text(
                locale,
                "Attached a review date to a decision.",
                "为一次决策设置了复盘日期。",
            ),
        });
    }

    let mut bias_signals = Vec::new();
    if decision_count > 0 && reviewed_decision_count == 0 {
        bias_signals.push(text(
            locale,
            "Decisions are being recorded without review dates; add explicit feedback loops.",
            "已有决策记录但没有复盘日期；请补上明确反馈闭环。",
        ));
    }
    if memo_count > 0 && memo_with_risk_count * 2 < memo_count {
        bias_signals.push(text(
            locale,
            "Several memos lack risk or disconfirming evidence; watch confirmation bias.",
            "部分备忘录缺少风险或反证条件；警惕确认偏差。",
        ));
    }
    if positions.iter().any(|position| position.weight > 0.4) {
        bias_signals.push(text(
            locale,
            "A single position is above 40% of tracked portfolio value; verify sizing rules.",
            "单一持仓超过已跟踪组合市值的 40%；请检查仓位规则。",
        ));
    }

    let mut rule_events = Vec::new();
    if locale.is_zh() {
        rule_events.push(format!("已记录 {memo_count} 份备忘录"));
        rule_events.push(format!("已记录 {decision_count} 次决策"));
        rule_events.push(format!("已跟踪 {} 个持仓", positions.len()));
    } else {
        rule_events.push(format!("{memo_count} memos logged"));
        rule_events.push(format!("{decision_count} decisions recorded"));
        rule_events.push(format!("{} positions tracked", positions.len()));
    }

    Ok(InvestorProfile {
        level,
        xp,
        next_level_xp,
        attributes,
        badges,
        bias_signals,
        rule_events,
    })
}

async fn count(pool: &SqlitePool, query: &str) -> AppResult<u32> {
    let row = sqlx::query(query).fetch_one(pool).await?;
    let value = row.try_get::<i64, _>("count")?;
    Ok(value.max(0) as u32)
}

fn bounded_score(value: u32) -> u32 {
    value.min(100)
}

fn text(locale: Locale, en: &str, zh: &str) -> String {
    if locale.is_zh() {
        zh.to_string()
    } else {
        en.to_string()
    }
}
