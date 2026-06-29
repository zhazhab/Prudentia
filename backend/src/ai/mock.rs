use async_trait::async_trait;

use crate::{
    ai::{AiError, AiProvider, InvestmentSystemRefinement, MemoExtraction},
    investment_system::InvestmentSystem,
    locale::Locale,
    memo::Memo,
};

pub struct MockAiProvider;

#[async_trait]
impl AiProvider for MockAiProvider {
    async fn extract_memo(&self, memo: &Memo, locale: Locale) -> Result<MemoExtraction, AiError> {
        let fallback = if memo.notes.trim().is_empty() {
            if locale.is_zh() {
                format!("用一份有纪律的检查清单复盘 {}。", memo.title)
            } else {
                format!("Review {} with a disciplined checklist.", memo.title)
            }
        } else {
            memo.notes.trim().to_string()
        };

        if locale.is_zh() {
            Ok(MemoExtraction {
                thesis: non_empty_or(
                    &memo.thesis,
                    format!("核心 thesis：{}", first_sentence(&fallback)),
                ),
                risks: non_empty_or(
                    &memo.risks,
                    "关键风险：估值、执行、竞争压力，以及 thesis 漂移。".to_string(),
                ),
                catalysts: non_empty_or(
                    &memo.catalysts,
                    "潜在催化剂：业绩、产品里程碑、资本配置和市场预期重估。".to_string(),
                ),
                disconfirming_evidence: non_empty_or(
                    &memo.disconfirming_evidence,
                    "反证条件：基本面偏离最初的投资假设。".to_string(),
                ),
                checklist: vec![
                    "行动前写下 base rate 假设".to_string(),
                    "加仓或建仓前明确退出条件".to_string(),
                    "按 thesis 周期设置复盘日期".to_string(),
                ],
            })
        } else {
            Ok(MemoExtraction {
                thesis: non_empty_or(
                    &memo.thesis,
                    format!("Core thesis: {}", first_sentence(&fallback)),
                ),
                risks: non_empty_or(
                    &memo.risks,
                    "Key risks: valuation, execution, competitive pressure, and thesis drift."
                        .to_string(),
                ),
                catalysts: non_empty_or(
                    &memo.catalysts,
                    "Potential catalysts: earnings, product milestones, capital allocation, and sentiment reset."
                        .to_string(),
                ),
                disconfirming_evidence: non_empty_or(
                    &memo.disconfirming_evidence,
                    "Disconfirming evidence: fundamentals miss the original underwriting assumptions."
                        .to_string(),
                ),
                checklist: vec![
                    "State the base-rate assumption before acting".to_string(),
                    "Name the kill criteria before sizing the position".to_string(),
                    "Schedule a review date tied to the thesis horizon".to_string(),
                ],
            })
        }
    }

    async fn refine_system(
        &self,
        system: &InvestmentSystem,
        locale: Locale,
    ) -> Result<InvestmentSystemRefinement, AiError> {
        let principles = if system.principles.is_empty() {
            default_principles(locale)
        } else {
            system.principles.clone()
        };

        let checklist_items = if system.checklist_items.is_empty() {
            default_checklist(locale)
        } else {
            system.checklist_items.clone()
        };

        Ok(InvestmentSystemRefinement {
            principles,
            checklist_items,
            circle_of_competence: system.circle_of_competence.clone(),
            decision_rules: system.decision_rules.clone(),
            summary: if locale.is_zh() {
                "Mock 整理：你的体系强调明确书写 thesis、命名风险，并建立复盘纪律。".to_string()
            } else {
                "Mock refinement: your system emphasizes explicit thesis writing, risk naming, and review discipline.".to_string()
            },
        })
    }
}

fn default_principles(locale: Locale) -> Vec<String> {
    if locale.is_zh() {
        vec![
            "先写 thesis，再看价格走势".to_string(),
            "优先记录反证证据，而不是确认性安慰".to_string(),
        ]
    } else {
        vec![
            "Write the thesis before checking price action".to_string(),
            "Prefer disconfirming evidence over confirming comfort".to_string(),
        ]
    }
}

fn default_checklist(locale: Locale) -> Vec<String> {
    if locale.is_zh() {
        vec![
            "这笔投资成立必须满足什么条件？".to_string(),
            "什么情况会证明我错了？".to_string(),
            "仓位是否匹配不确定性？".to_string(),
        ]
    } else {
        vec![
            "What has to be true for this investment to work?".to_string(),
            "What would prove me wrong?".to_string(),
            "Is position size consistent with uncertainty?".to_string(),
        ]
    }
}

fn non_empty_or(value: &str, fallback: String) -> String {
    if value.trim().is_empty() {
        fallback
    } else {
        value.to_string()
    }
}

fn first_sentence(value: &str) -> String {
    value
        .split('.')
        .next()
        .unwrap_or(value)
        .trim()
        .chars()
        .take(180)
        .collect()
}
