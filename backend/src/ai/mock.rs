use std::path::Path;

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::{
    ai::{
        AiError, AiProvider, AiProviderEvent, ConversationContext, ConversationProjection,
        InvestmentSystemRefinement, MemoChatContext, MemoExtraction, PortfolioReviewContext,
        ResearchAnalysis, ResearchSourceInput, StockSnapshotContext,
    },
    investment_system::InvestmentSystem,
    locale::Locale,
    memo::Memo,
    portfolio::{PortfolioImageDraftRow, PortfolioImageRecognition},
};

pub struct MockAiProvider;

#[async_trait]
impl AiProvider for MockAiProvider {
    async fn respond_to_conversation(
        &self,
        context: &ConversationContext,
        locale: Locale,
        events: mpsc::UnboundedSender<AiProviderEvent>,
    ) -> Result<String, AiError> {
        let message = first_sentence(context.user_message.trim());
        let response = if locale.is_zh() {
            format!("我听到了：{message}。我们可以继续把事实、判断、风险和待验证问题分开讨论。")
        } else {
            format!("I heard: {message}. We can separate facts, judgments, risks, and open questions as we continue.")
        };
        let _ = events.send(AiProviderEvent::Stage {
            provider: "mock".to_string(),
            stage: "generating".to_string(),
        });
        let _ = events.send(AiProviderEvent::TextDelta(response.clone()));
        Ok(response)
    }

    async fn project_conversation(
        &self,
        context: &ConversationContext,
        _assistant_response: &str,
        _locale: Locale,
    ) -> Result<ConversationProjection, AiError> {
        Ok(ConversationProjection {
            summary: first_sentence(&context.user_message),
            actions: Vec::new(),
        })
    }

    async fn respond_to_memo_chat(
        &self,
        context: &MemoChatContext,
        locale: Locale,
    ) -> Result<String, AiError> {
        let message = first_sentence(context.user_message.trim());
        if locale.is_zh() {
            Ok(format!(
                "我听到了：{}。我们可以先把问题拆成假设、证据、风险和需要继续验证的点。",
                message
            ))
        } else {
            Ok(format!(
                "I heard: {}. We can break this into hypothesis, evidence, risks, and open checks.",
                message
            ))
        }
    }

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

    async fn distill_research_source(
        &self,
        input: &ResearchSourceInput,
        locale: Locale,
    ) -> Result<ResearchAnalysis, AiError> {
        let summary = if locale.is_zh() {
            format!(
                "Mock 研究整理：{} 强调把判断写清楚并用检查清单复盘。",
                input.title
            )
        } else {
            format!(
                "Mock research distillation: {} emphasizes clear judgment and checklist review.",
                input.title
            )
        };

        Ok(mock_research_analysis(locale, summary))
    }

    async fn analyze_stock_snapshot(
        &self,
        context: &StockSnapshotContext,
        locale: Locale,
    ) -> Result<ResearchAnalysis, AiError> {
        let summary = if locale.is_zh() {
            format!(
                "Mock 股票快照：{} 需要结合 thesis、仓位和报价复盘。",
                context.symbol
            )
        } else {
            format!(
                "Mock stock snapshot: {} should be reviewed against thesis, position, and quote context.",
                context.symbol
            )
        };

        Ok(mock_research_analysis(locale, summary))
    }

    async fn review_portfolio_risk(
        &self,
        context: &PortfolioReviewContext,
        locale: Locale,
    ) -> Result<ResearchAnalysis, AiError> {
        let summary = if locale.is_zh() {
            format!(
                "Mock 组合复盘：{} 个持仓需要按风险来源归类。",
                context.positions.len()
            )
        } else {
            format!(
                "Mock portfolio review: {} positions should be grouped by source of risk.",
                context.positions.len()
            )
        };

        Ok(mock_research_analysis(locale, summary))
    }

    async fn recognize_portfolio_image(
        &self,
        _image_path: &Path,
    ) -> Result<PortfolioImageRecognition, AiError> {
        Ok(PortfolioImageRecognition {
            rows: vec![PortfolioImageDraftRow {
                symbol: "AAPL".to_string(),
                name: "Apple Inc.".to_string(),
                quantity: "12".to_string(),
                average_cost: "150.25".to_string(),
                currency: "USD".to_string(),
                account: Some("Main".to_string()),
                market: Some("US".to_string()),
                sector: None,
                imported_market_value: Some("2450.00".to_string()),
                last_price: Some("204.1667".to_string()),
                notes: Some("Recognized from mock image provider.".to_string()),
                confidence: "high".to_string(),
                warnings: Vec::new(),
            }],
            warnings: vec!["Mock image recognition preview.".to_string()],
        })
    }
}

fn mock_research_analysis(locale: Locale, summary: String) -> ResearchAnalysis {
    if locale.is_zh() {
        ResearchAnalysis {
            summary,
            insights: vec!["把事实、判断和待验证假设分开记录。".to_string()],
            risks: vec!["主要风险来自 thesis 漂移、估值和执行偏差。".to_string()],
            checklist: vec!["复盘前先写下最重要的反证信号。".to_string()],
            candidate_principles: vec!["先定义可证伪条件，再形成结论。".to_string()],
            candidate_checklist_items: vec!["什么事实会推翻当前判断？".to_string()],
        }
    } else {
        ResearchAnalysis {
            summary,
            insights: vec!["Separate facts, judgments, and hypotheses to verify.".to_string()],
            risks: vec![
                "Key risks include thesis drift, valuation pressure, and execution misses."
                    .to_string(),
            ],
            checklist: vec![
                "Name the most important disconfirming signal before review.".to_string(),
            ],
            candidate_principles: vec![
                "Define falsifiable conditions before forming the conclusion.".to_string(),
            ],
            candidate_checklist_items: vec![
                "What fact would overturn the current view?".to_string()
            ],
        }
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
