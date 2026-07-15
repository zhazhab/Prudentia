use super::{ThreadSubject, ThreadSubjectKind};
use crate::ai::runtime::TaskComplexity;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskRouteReason {
    SocialTurn,
    ShortQuestion,
    SubjectClarification,
    AttachmentAnalysis,
    InvestmentSystem,
    MultiPartRequest,
    LongRequest,
    ExplicitDeepAnalysis,
    CompanyResearch,
    StandardConversation,
}

impl TaskRouteReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SocialTurn => "social_turn",
            Self::ShortQuestion => "short_question",
            Self::SubjectClarification => "subject_clarification",
            Self::AttachmentAnalysis => "attachment_analysis",
            Self::InvestmentSystem => "investment_system",
            Self::MultiPartRequest => "multi_part_request",
            Self::LongRequest => "long_request",
            Self::ExplicitDeepAnalysis => "explicit_deep_analysis",
            Self::CompanyResearch => "company_research",
            Self::StandardConversation => "standard_conversation",
        }
    }
}

pub fn subject_clarification_assessment() -> TaskAssessment {
    assessment(
        TaskComplexity::Simple,
        TaskRouteReason::SubjectClarification,
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskAssessment {
    pub complexity: TaskComplexity,
    pub reason: TaskRouteReason,
}

pub fn assess_task(
    message: &str,
    subject: &ThreadSubject,
    has_attachments: bool,
    research_planned: bool,
) -> TaskAssessment {
    if has_attachments {
        return assessment(TaskComplexity::Deep, TaskRouteReason::AttachmentAnalysis);
    }
    if super::is_simple_social_turn(message) {
        return assessment(TaskComplexity::Simple, TaskRouteReason::SocialTurn);
    }
    if subject.kind_type() == ThreadSubjectKind::InvestmentSystem {
        return assessment(TaskComplexity::Deep, TaskRouteReason::InvestmentSystem);
    }
    if is_multi_part_request(message) {
        return assessment(TaskComplexity::Deep, TaskRouteReason::MultiPartRequest);
    }
    if message.chars().count() >= 320 {
        return assessment(TaskComplexity::Deep, TaskRouteReason::LongRequest);
    }
    if requests_deep_analysis(message) {
        return assessment(TaskComplexity::Deep, TaskRouteReason::ExplicitDeepAnalysis);
    }
    if subject.kind_type() == ThreadSubjectKind::Company && research_planned {
        return assessment(TaskComplexity::Standard, TaskRouteReason::CompanyResearch);
    }
    if subject.kind_type() == ThreadSubjectKind::General
        && !research_planned
        && is_short_non_material_turn(message)
    {
        return assessment(TaskComplexity::Simple, TaskRouteReason::ShortQuestion);
    }
    assessment(
        TaskComplexity::Standard,
        TaskRouteReason::StandardConversation,
    )
}

fn assessment(complexity: TaskComplexity, reason: TaskRouteReason) -> TaskAssessment {
    TaskAssessment { complexity, reason }
}

fn is_multi_part_request(message: &str) -> bool {
    let numbered_lines = message
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            let Some(first) = trimmed.chars().next() else {
                return false;
            };
            first.is_ascii_digit()
                && trimmed
                    .chars()
                    .nth(1)
                    .is_some_and(|character| matches!(character, '.' | '、' | ')' | '）'))
        })
        .count();
    numbered_lines >= 2 || message.matches(['?', '？']).count() >= 2
}

fn requests_deep_analysis(message: &str) -> bool {
    if super::requests_local_context_only(message) && message.chars().count() <= 160 {
        return false;
    }
    let normalized = message.to_ascii_lowercase();
    let explicit_depth = [
        "深入",
        "全面",
        "详细",
        "完整",
        "逐项",
        "系统性",
        "deep",
        "comprehensive",
        "detailed",
        "thorough",
    ];
    if explicit_depth
        .iter()
        .any(|keyword| normalized.contains(keyword))
    {
        return true;
    }

    let inherently_deep_topics = [
        "财报",
        "年报",
        "季报",
        "估值",
        "反证",
        "风险",
        "护城河",
        "竞争壁垒",
        "商业模式",
        "怎么赚钱",
        "如何赚钱",
        "filing",
        "earnings",
        "valuation",
        "disconfirm",
        "risk",
        "moat",
        "business model",
        "revenue model",
    ];
    if inherently_deep_topics
        .iter()
        .any(|keyword| normalized.contains(keyword))
    {
        return true;
    }

    let complex_topics = ["情景", "对比", "scenario", "compare"];
    complex_topics
        .iter()
        .filter(|keyword| normalized.contains(**keyword))
        .count()
        >= 2
}

fn is_short_non_material_turn(message: &str) -> bool {
    if message.chars().count() > 80 {
        return false;
    }
    let normalized = message.to_ascii_lowercase();
    let material_keywords = [
        "买入",
        "卖出",
        "交易",
        "记录",
        "更新",
        "修改",
        "规则",
        "持仓",
        "收益",
        "分析",
        "研究",
        "buy",
        "sell",
        "trade",
        "record",
        "update",
        "rule",
        "portfolio",
        "return",
        "analyze",
        "research",
    ];
    !material_keywords
        .iter()
        .any(|keyword| normalized.contains(keyword))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subject(kind: &str) -> ThreadSubject {
        ThreadSubject {
            kind: kind.to_string(),
            subject_key: None,
            label: None,
            confidence: 1.0,
        }
    }

    #[test]
    fn social_turns_use_the_simple_tier() {
        assert_eq!(
            assess_task("你好", &subject("general"), false, false),
            TaskAssessment {
                complexity: TaskComplexity::Simple,
                reason: TaskRouteReason::SocialTurn
            }
        );
    }

    #[test]
    fn subject_confirmation_uses_the_simple_tier() {
        assert_eq!(
            subject_clarification_assessment(),
            TaskAssessment {
                complexity: TaskComplexity::Simple,
                reason: TaskRouteReason::SubjectClarification
            }
        );
    }

    #[test]
    fn ordinary_company_research_uses_the_standard_tier() {
        assert_eq!(
            assess_task("分析一下 PDD", &subject("company"), false, true),
            TaskAssessment {
                complexity: TaskComplexity::Standard,
                reason: TaskRouteReason::CompanyResearch
            }
        );
    }

    #[test]
    fn attachments_and_explicit_deep_analysis_use_the_deep_tier() {
        assert_eq!(
            assess_task("看看这个文件", &subject("company"), true, false)
                .reason
                .as_str(),
            "attachment_analysis"
        );
        assert_eq!(
            assess_task(
                "深入分析最新财报、估值、风险和反证",
                &subject("company"),
                false,
                true
            ),
            TaskAssessment {
                complexity: TaskComplexity::Deep,
                reason: TaskRouteReason::ExplicitDeepAnalysis
            }
        );
        assert_eq!(
            assess_task("PDD 最新财报怎么看", &subject("company"), false, true).complexity,
            TaskComplexity::Deep
        );
    }

    #[test]
    fn investment_system_changes_use_the_deep_tier() {
        assert_eq!(
            assess_task(
                "把卖出规则改成可执行节点",
                &subject("investment_system"),
                false,
                false
            ),
            TaskAssessment {
                complexity: TaskComplexity::Deep,
                reason: TaskRouteReason::InvestmentSystem
            }
        );
        assert_eq!(
            assess_task("你好", &subject("investment_system"), false, false).complexity,
            TaskComplexity::Simple
        );
    }

    #[test]
    fn explicit_risk_analysis_uses_the_deep_tier() {
        assert_eq!(
            assess_task("分析 PDD 的风险", &subject("company"), false, true).complexity,
            TaskComplexity::Deep
        );
    }

    #[test]
    fn focused_local_company_view_question_uses_the_standard_tier() {
        assert_eq!(
            assess_task(
                "只根据已经沉淀的公司看法，指出最关键的反证，不增加新事实。",
                &subject("company"),
                false,
                false
            ),
            TaskAssessment {
                complexity: TaskComplexity::Standard,
                reason: TaskRouteReason::StandardConversation
            }
        );
    }

    #[test]
    fn moat_analysis_uses_the_deep_tier() {
        assert_eq!(
            assess_task("PDD 真正的护城河是什么？", &subject("company"), false, true).complexity,
            TaskComplexity::Deep
        );
    }

    #[test]
    fn business_model_analysis_uses_the_deep_tier() {
        assert_eq!(
            assess_task("PDD 的商业模式是什么？", &subject("company"), false, true).complexity,
            TaskComplexity::Deep
        );
    }

    #[test]
    fn multi_part_requests_use_the_deep_tier() {
        assert_eq!(
            assess_task(
                "1. 分析竞争格局\n2. 对比估值\n3. 列出反证",
                &subject("company"),
                false,
                true
            )
            .reason
            .as_str(),
            "multi_part_request"
        );
    }
}
