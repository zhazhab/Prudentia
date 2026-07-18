use serde::{Deserialize, Serialize};

use super::super::types::{ThreadSubject, ThreadSubjectKind};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum EvidenceCategory {
    Official,
    Independent,
    Community,
}

impl EvidenceCategory {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Official => "official filings",
            Self::Independent => "independent analysis",
            Self::Community => "community viewpoints",
        }
    }

    pub(super) fn source_tier(self) -> &'static str {
        match self {
            Self::Official => "primary",
            Self::Independent => "secondary",
            Self::Community => "community",
        }
    }

    fn query_focus(self) -> &'static str {
        match self {
            Self::Official => "annual filing cash conversion owner earnings per diluted share maintenance capex growth capex share based compensation dilution management compensation succession capital allocation acquisitions buybacks debt dividends liquidity debt maturities contingent liabilities",
            Self::Independent => "historical stability industry change bargaining power competitor response bottlenecks moat maintenance cost entrant replacement cost incremental return on invested capital return on retained earnings reinvestment runway management candor integrity incentives capital allocation financial resilience",
            Self::Community => "customer and supplier incentives employee channel incentives switching multi-homing subsidies service operations competitor threat management culture recurring complaints popular fundamental discussion",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct ResearchQuery {
    pub(super) category: EvidenceCategory,
    pub(super) text: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ResearchPlan {
    subject: CompanyResearchSubject,
    intent: CompanyResearchIntent,
    annual_history_years: Option<u8>,
    queries: [ResearchQuery; 3],
}

impl ResearchPlan {
    pub(super) fn company_name(&self) -> &str {
        &self.subject.company_name
    }

    pub(super) fn symbol(&self) -> &str {
        &self.subject.symbol
    }

    pub(super) fn base_symbol(&self) -> String {
        self.subject
            .symbol
            .split('.')
            .next()
            .unwrap_or(self.subject.symbol.as_str())
            .to_ascii_uppercase()
    }

    pub(super) fn queries(&self) -> &[ResearchQuery; 3] {
        &self.queries
    }

    pub(super) fn annual_history_years(&self) -> Option<usize> {
        self.annual_history_years.map(usize::from)
    }

    pub(super) fn query_cache_identity(&self) -> String {
        serde_json::to_string(&QueryCacheIdentity {
            subject: &self.subject,
            intent: self.intent,
            annual_history_years: self.annual_history_years,
            queries: &self.queries,
        })
        .expect("research query cache identity is serializable")
    }

    pub(super) fn subject_cache_identity(&self) -> String {
        let evidence_categories = self
            .queries
            .iter()
            .map(|query| query.category)
            .collect::<Vec<_>>();
        serde_json::to_string(&SubjectCacheIdentity {
            subject: &self.subject,
            evidence_categories,
            annual_history_years: self.annual_history_years,
        })
        .expect("research subject cache identity is serializable")
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CompanyResearchSubject {
    company_name: String,
    symbol: String,
}

#[derive(Serialize)]
struct QueryCacheIdentity<'a> {
    subject: &'a CompanyResearchSubject,
    intent: CompanyResearchIntent,
    #[serde(skip_serializing_if = "Option::is_none")]
    annual_history_years: Option<u8>,
    queries: &'a [ResearchQuery; 3],
}

#[derive(Serialize)]
struct SubjectCacheIdentity<'a> {
    subject: &'a CompanyResearchSubject,
    evidence_categories: Vec<EvidenceCategory>,
    #[serde(skip_serializing_if = "Option::is_none")]
    annual_history_years: Option<u8>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum CompanyResearchIntent {
    BusinessModel,
    Moat,
    Earnings,
    News,
    Risk,
    Fundamentals,
    General,
}

impl CompanyResearchIntent {
    fn query_terms(self) -> &'static str {
        match self {
            Self::BusinessModel => "business model products users customers and payers value chain revenue streams cost structure unit economics profit pool value capture pricing power take rate competitive intensity competitors substitutes structural moat earnings predictability five to ten year earnings power",
            Self::Moat => "structural moat brand pricing power network effects scale economies cost curve switching costs customer retention repeat purchase churn merchant economics take rate contribution margin marketing efficiency quality complaints refunds protected intellectual property exclusive resources regulatory licenses market share without subsidies replication risk competitor attack response founder dependence channel change durable excess returns earnings predictability five to ten year earnings power",
            Self::Earnings => "latest earnings financial results owner earnings cash conversion dilution capital allocation",
            Self::News => "latest company news announcement",
            Self::Risk => "current business risks competition regulation ruin risk refinancing off balance sheet obligations management incentives succession",
            Self::Fundamentals => "current fundamentals growth margins cash flow owner earnings return on invested capital reinvestment earnings predictability capital allocation financial resilience",
            Self::General => "current company analysis business model customers and payers value chain unit economics profit pool competitive intensity structural moat owner economics reinvestment management incentives capital allocation financial resilience earnings predictability five to ten year earnings power",
        }
    }
}

pub(in crate::conversation) fn plan_research(
    message: &str,
    subject: &ThreadSubject,
) -> Option<ResearchPlan> {
    if subject.kind_type() != ThreadSubjectKind::Company
        || super::super::is_simple_social_turn(message)
        || super::super::requests_local_context_only(message)
    {
        return None;
    }

    let intent = classify_intent(message);
    let symbol = subject.subject_key.as_deref().unwrap_or_default();
    let label = subject.label.as_deref().unwrap_or(symbol);
    let company = CompanyResearchSubject {
        company_name: label.to_string(),
        symbol: symbol.to_string(),
    };
    let annual_history_years = detect_annual_history_years(message).or_else(|| {
        matches!(
            intent,
            CompanyResearchIntent::BusinessModel
                | CompanyResearchIntent::Moat
                | CompanyResearchIntent::General
        )
        .then_some(5)
    });
    let queries = research_queries(&company, intent, annual_history_years);
    Some(ResearchPlan {
        subject: company,
        intent,
        annual_history_years,
        queries,
    })
}

pub(in crate::conversation) fn is_community_insights_request(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    contains_any(
        &normalized,
        &[
            "社区怎么看",
            "社区看法",
            "社区观点",
            "社区看点",
            "社区讨论",
            "热门帖子",
            "热门讨论",
            "热帖",
            "投资社区",
            "投资者怎么看",
            "投资者观点",
            "投资者讨论",
            "雪球怎么看",
            "雪球观点",
            "雪球讨论",
            "股吧讨论",
            "reddit",
            "tradingview",
            "community insights",
            "community views",
            "community discussion",
            "community sentiment",
            "investor discussion",
            "popular posts",
        ],
    )
}

pub(in crate::conversation) fn plan_community_insights(
    message: &str,
    subject: &ThreadSubject,
) -> Option<ResearchPlan> {
    if !is_community_insights_request(message) {
        return None;
    }
    plan_community_research(message, subject)
}

pub(in crate::conversation) fn plan_community_research(
    message: &str,
    subject: &ThreadSubject,
) -> Option<ResearchPlan> {
    let mut plan = plan_research(message, subject)?;
    plan.annual_history_years = None;
    plan.queries = research_queries(&plan.subject, plan.intent, None);
    Some(plan)
}

pub(in crate::conversation) fn community_request_requires_company_research(message: &str) -> bool {
    is_community_insights_request(message)
        && classify_intent(message) != CompanyResearchIntent::General
}

fn classify_intent(message: &str) -> CompanyResearchIntent {
    let normalized = message.to_ascii_lowercase();
    if contains_any(
        &normalized,
        &[
            "商业模式",
            "怎么赚钱",
            "如何赚钱",
            "挣钱",
            "赚钱难",
            "盈利难度",
            "议价权",
            "竞争强度",
            "business model",
            "revenue model",
            "value chain",
            "monetization",
            "competitive intensity",
            "bargaining power",
            "profit pool",
            "profit difficulty",
        ],
    ) {
        CompanyResearchIntent::BusinessModel
    } else if contains_any(
        &normalized,
        &[
            "护城河",
            "竞争壁垒",
            "moat",
            "durable competitive advantage",
        ],
    ) {
        CompanyResearchIntent::Moat
    } else if contains_any(&normalized, &["财报", "业绩", "earnings", "filing"]) {
        CompanyResearchIntent::Earnings
    } else if contains_any(&normalized, &["新闻", "公告", "news", "announcement"]) {
        CompanyResearchIntent::News
    } else if contains_any(
        &normalized,
        &["风险", "竞争", "risk", "competition", "regulation"],
    ) {
        CompanyResearchIntent::Risk
    } else if contains_any(
        &normalized,
        &[
            "基本面",
            "增长",
            "利润",
            "收入",
            "估值",
            "fundamentals",
            "growth",
            "margin",
            "revenue",
            "valuation",
        ],
    ) {
        CompanyResearchIntent::Fundamentals
    } else {
        CompanyResearchIntent::General
    }
}

pub(in crate::conversation) fn company_research_scope(message: &str) -> &'static str {
    match classify_intent(message) {
        CompanyResearchIntent::BusinessModel => "business_model",
        CompanyResearchIntent::Moat => "moat",
        CompanyResearchIntent::Earnings => "earnings",
        CompanyResearchIntent::News => "news",
        CompanyResearchIntent::Risk => "risk",
        CompanyResearchIntent::Fundamentals => "fundamentals",
        CompanyResearchIntent::General => "default",
    }
}

fn research_queries(
    company: &CompanyResearchSubject,
    intent: CompanyResearchIntent,
    annual_history_years: Option<u8>,
) -> [ResearchQuery; 3] {
    let subject = subject_terms(company);
    let intent_focus = intent.query_terms();
    let focus = annual_history_years
        .map(|years| {
            format!("{intent_focus} last {years} annual reports historical revenue net income operating cash flow")
        })
        .unwrap_or_else(|| intent_focus.to_string());
    [
        ResearchQuery {
            category: EvidenceCategory::Official,
            text: format!(
                "{subject} {focus} {} official investor relations earnings announcement",
                EvidenceCategory::Official.query_focus()
            ),
        },
        ResearchQuery {
            category: EvidenceCategory::Independent,
            text: format!(
                "{subject} {focus} {} independent analysis risks competition operations",
                EvidenceCategory::Independent.query_focus()
            ),
        },
        ResearchQuery {
            category: EvidenceCategory::Community,
            text: format!(
                "{subject} {focus} {} business economics customers competition margins comments site:xueqiu.com OR site:reddit.com OR site:moomoo.com OR site:guba.eastmoney.com",
                EvidenceCategory::Community.query_focus()
            ),
        },
    ]
}

fn detect_annual_history_years(message: &str) -> Option<u8> {
    let normalized = message.to_ascii_lowercase();
    let mut calendar_years = normalized
        .as_bytes()
        .windows(4)
        .filter(|window| window.iter().all(u8::is_ascii_digit))
        .filter_map(|window| std::str::from_utf8(window).ok()?.parse::<u16>().ok())
        .filter(|year| (1990..=2100).contains(year))
        .collect::<Vec<_>>();
    calendar_years.sort_unstable();
    calendar_years.dedup();
    if let (Some(first), Some(last)) = (calendar_years.first(), calendar_years.last()) {
        let years = last.saturating_sub(*first).saturating_add(1);
        if (2..=10).contains(&years) {
            return Some(years as u8);
        }
    }

    for (token, years) in [
        ("十年", 10),
        ("九年", 9),
        ("八年", 8),
        ("七年", 7),
        ("六年", 6),
        ("五年", 5),
        ("四年", 4),
        ("三年", 3),
        ("两年", 2),
        ("二年", 2),
    ] {
        if normalized.contains(token) {
            return Some(years);
        }
    }
    for years in 2..=10 {
        if [
            format!("{years}年"),
            format!("{years} 年"),
            format!("{years} years"),
            format!("{years}-year"),
        ]
        .iter()
        .any(|pattern| normalized.contains(pattern))
        {
            return Some(years);
        }
    }
    contains_any(
        &normalized,
        &["历年财报", "多年财报", "historical financials"],
    )
    .then_some(5)
}

fn subject_terms(company: &CompanyResearchSubject) -> String {
    if company.company_name.eq_ignore_ascii_case(&company.symbol) || company.symbol.is_empty() {
        company.company_name.clone()
    } else {
        format!("{} {}", company.company_name, company.symbol)
    }
}

fn contains_any(value: &str, candidates: &[&str]) -> bool {
    candidates.iter().any(|candidate| value.contains(candidate))
}

#[cfg(test)]
mod tests {
    use super::{
        is_community_insights_request, plan_community_insights, plan_research, EvidenceCategory,
    };
    use crate::conversation::types::ThreadSubject;

    fn company_subject(symbol: &str, name: &str) -> ThreadSubject {
        ThreadSubject {
            kind: "company".to_string(),
            subject_key: Some(symbol.to_string()),
            label: Some(name.to_string()),
            confidence: 0.95,
        }
    }

    #[test]
    fn effective_turn_company_controls_queries_and_cache_identity() {
        let tencent = plan_research("分析一下腾讯", &company_subject("0700.HK", "腾讯控股"))
            .expect("Tencent plan");
        let pdd = plan_research("分析一下拼多多", &company_subject("PDD", "PDD Holdings"))
            .expect("PDD plan");

        assert_eq!(tencent.symbol(), "0700.HK");
        assert!(tencent
            .queries()
            .iter()
            .all(|query| query.text.contains("腾讯控股 0700.HK") && !query.text.contains("PDD")));
        assert_ne!(
            tencent.subject_cache_identity(),
            pdd.subject_cache_identity()
        );
        assert_ne!(tencent.query_cache_identity(), pdd.query_cache_identity());
    }

    #[test]
    fn substantive_company_turns_produce_an_executable_plan() {
        let company = company_subject("PDD", "PDD Holdings");
        assert!(plan_research("分析一下 PDD", &company).is_some());
        assert!(plan_research("What do you think about PDD's margins?", &company).is_some());
        assert!(plan_research("PDD 值得买吗？", &company).is_some());
        assert!(plan_research("你好", &company).is_none());
        assert!(plan_research("只根据已经沉淀的公司看法回答，不增加新事实。", &company).is_none());
        assert!(plan_research("分析一下我的持仓", &ThreadSubject::default()).is_none());
    }

    #[test]
    fn community_insight_requests_are_explicit_and_do_not_match_general_analysis() {
        for message in [
            "看看腾讯最近的社区观点",
            "整理腾讯的社区看点和热帖",
            "雪球怎么看拼多多？",
            "Find popular Reddit investor discussions about Micron",
        ] {
            assert!(is_community_insights_request(message), "{message}");
        }
        for message in ["分析腾讯", "腾讯的商业模式是什么？", "查看腾讯最新财报"]
        {
            assert!(!is_community_insights_request(message), "{message}");
        }
    }

    #[test]
    fn community_insight_plan_does_not_inherit_default_financial_history() {
        let plan =
            plan_community_insights("社区怎么看 PDD？", &company_subject("PDD", "PDD Holdings"))
                .expect("community plan");

        assert_eq!(plan.annual_history_years(), None);
        assert!(!plan.queries()[2].text.contains("last 5 annual reports"));
        assert!(plan.queries()[2].text.contains("site:xueqiu.com"));
    }

    #[test]
    fn plan_contains_typed_queries_without_private_turn_data() {
        let plan = plan_research(
            "我持有 587 股，请搜索 PDD 最新财报",
            &company_subject("PDD", "PDD Holdings"),
        )
        .expect("research plan");

        assert_eq!(plan.queries()[0].category, EvidenceCategory::Official);
        assert_eq!(plan.queries()[1].category, EvidenceCategory::Independent);
        assert_eq!(plan.queries()[2].category, EvidenceCategory::Community);
        assert!(plan
            .queries()
            .iter()
            .all(|query| query.text.contains("PDD Holdings PDD")));
        assert!(plan
            .queries()
            .iter()
            .all(|query| query.text.contains("latest earnings")));
        assert!(plan
            .queries()
            .iter()
            .all(|query| !query.text.contains("587")));
    }

    #[test]
    fn business_model_requests_plan_for_value_chain_and_monetization_evidence() {
        let plan = plan_research(
            "PDD 的商业模式是什么，怎么赚钱？",
            &company_subject("PDD", "PDD Holdings"),
        )
        .expect("research plan");
        let queries = plan
            .queries()
            .iter()
            .map(|query| query.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        assert!(queries.contains("business model"));
        assert!(queries.contains("customers and payers"));
        assert!(queries.contains("revenue streams"));
        assert!(queries.contains("cost structure"));
        assert!(queries.contains("competitive intensity"));
        assert!(queries.contains("bargaining power"));
        assert!(queries.contains("profit pool"));
        assert!(queries.contains("pricing power"));
        assert!(queries.contains("cash conversion"));
        assert!(queries.contains("customer and supplier incentives"));
        assert!(queries.contains("competitor response"));
        assert!(queries.contains("bottlenecks"));
        assert!(queries.contains("return on invested capital"));
        assert!(queries.contains("moat maintenance cost"));
        assert!(queries.contains("entrant replacement cost"));
        assert!(queries.contains("competitor threat"));
        assert!(queries.contains("five to ten year earnings power"));
        assert!(queries.contains("earnings predictability"));
        assert!(queries.contains("owner earnings per diluted share"));
        assert!(queries.contains("maintenance capex"));
        assert!(queries.contains("share based compensation dilution"));
        assert!(queries.contains("incremental return on invested capital"));
        assert!(queries.contains("return on retained earnings"));
        assert!(queries.contains("reinvestment runway"));
        assert!(queries.contains("management candor integrity"));
        assert!(queries.contains("capital allocation acquisitions buybacks debt dividends"));
        assert!(plan.queries()[0].text.contains("last 5 annual reports"));
        assert!(plan
            .queries()
            .iter()
            .all(|query| !query.text.contains("valuation")));
        assert!(plan
            .queries()
            .iter()
            .all(|query| !query.text.contains("stocktwits")));
        assert_eq!(plan.annual_history_years(), Some(5));
        assert!(plan.queries().iter().all(|query| query.text.len() <= 850));
    }

    #[test]
    fn valuation_wording_routes_to_company_fundamentals_without_market_queries() {
        let plan = plan_research("分析 PDD 当前估值", &company_subject("PDD", "PDD Holdings"))
            .expect("research plan");

        assert!(plan
            .queries()
            .iter()
            .all(|query| query.text.contains("current fundamentals")));
        assert!(plan
            .queries()
            .iter()
            .all(|query| !query.text.contains("valuation")));
    }

    #[test]
    fn moat_requests_plan_for_structural_and_counterfactual_evidence() {
        let plan = plan_research(
            "PDD 真正的护城河是什么？",
            &company_subject("PDD", "PDD Holdings"),
        )
        .expect("research plan");
        let queries = plan
            .queries()
            .iter()
            .map(|query| query.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        assert!(queries.contains("structural moat"));
        assert!(queries.contains("pricing power"));
        assert!(queries.contains("network effects"));
        assert!(queries.contains("scale economies"));
        assert!(queries.contains("switching costs"));
        assert!(queries.contains("without subsidies"));
        assert!(queries.contains("replication risk"));
        assert!(queries.contains("customer retention repeat purchase"));
        assert!(queries.contains("merchant economics"));
        assert!(queries.contains("contribution margin"));
        assert!(queries.contains("quality complaints refunds"));
        assert!(queries.contains("competitor attack response"));
        assert!(queries.contains("moat maintenance cost"));
        assert!(queries.contains("entrant replacement cost"));
        assert!(queries.contains("five to ten year earnings power"));
        assert!(queries.contains("earnings predictability"));
        assert!(queries.contains("management candor integrity incentives capital allocation"));
        assert!(queries.contains("incremental return on invested capital"));
        assert_eq!(plan.annual_history_years(), Some(5));
    }

    #[test]
    fn query_and_subject_cache_identities_capture_different_execution_scopes() {
        let pdd = company_subject("PDD", "PDD Holdings");
        let earnings = plan_research("分析 PDD 最新财报", &pdd).expect("earnings plan");
        let fundamentals = plan_research("分析 PDD 当前估值", &pdd).expect("fundamentals plan");
        let history = plan_research("研究 PDD 近五年财报", &pdd).expect("history plan");

        assert_ne!(
            earnings.query_cache_identity(),
            fundamentals.query_cache_identity()
        );
        assert_eq!(
            earnings.subject_cache_identity(),
            fundamentals.subject_cache_identity()
        );
        assert_ne!(
            earnings.query_cache_identity(),
            history.query_cache_identity()
        );
        assert_ne!(
            earnings.subject_cache_identity(),
            history.subject_cache_identity()
        );
        assert!(history.queries()[0].text.contains("5 annual reports"));
    }
}
