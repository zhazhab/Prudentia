use crate::ai::ConversationContext;

pub(super) fn response_structure(context: &ConversationContext) -> String {
    let structure = if is_focused_moat_analysis(context) {
        Some(FOCUSED_MOAT_ANALYSIS_STRUCTURE)
    } else if is_broad_company_analysis(context) {
        Some(BROAD_COMPANY_ANALYSIS_STRUCTURE)
    } else if is_focused_business_model_analysis(context) {
        Some(FOCUSED_BUSINESS_MODEL_ANALYSIS_STRUCTURE)
    } else {
        None
    };
    let mut response =
        structure.map_or_else(|| DEFAULT_RESPONSE_STRUCTURE.to_string(), str::to_string);
    if is_company_context(context) {
        response.push('\n');
        response.push_str(COMPANY_ONLY_SCOPE);
    }
    if structure.is_some() {
        response.push('\n');
        response.push_str(DURABLE_OWNER_AUDIT);
        response.push('\n');
        response.push_str(MANDATORY_INVESTOR_QUESTIONS);
    }
    response
}

const DEFAULT_RESPONSE_STRUCTURE: &str = r#"Unless the user explicitly asks for a detailed report, keep the answer under 800 Simplified Chinese characters or 350 English words. Answer the user's question directly and use no more than six concise bullets or short sections."#;

const COMPANY_ONLY_SCOPE: &str = r#"Company-only analysis scope for every company turn:
- Analyze the enterprise, not its traded security. In scope are products and services, customers and suppliers, industry structure, competitors, market share, operations, unit economics, financial statements, cash generation, reinvestment, capital allocation inside the business, management, governance, regulation, and operating risks.
- Capital-market data is out of scope. Never discuss or use current or historical share price, market quotation, market capitalization, enterprise value, valuation multiples, price targets, stock returns, chart or technical analysis, analyst ratings, the user's position size/cost/profit or loss, or buy/sell/hold implications.
- Industry and product-market evidence remains in scope; "market" here means customers, competitors, suppliers, and commercial demand, never the stock market.
- Ignore capital-market or portfolio details even if they appear in research sources, prior messages, summaries, or other context. Do not add Valuation, Stock View, Investment View, or Portfolio Impact sections. End with a company-quality and operating-uncertainty conclusion."#;

const BROAD_COMPANY_ANALYSIS_STRUCTURE: &str = r#"For broad company analysis, target 4,000 to 5,000 Simplified Chinese characters or 1,800 to 2,300 English words when the evidence supports it. This is a depth target, not permission for generic filler or repeated methodology. Prefer causal chains, concrete operating examples, and decision quantities over adjectives.
Business model is the first and most important section. It must receive at least 60% of the substantive analysis and normally 2,400 to 3,000 Simplified Chinese characters when primary evidence is available. Do not compress it to make room for later sections. Start with a clearly labeled Business Model section, never with a financial summary. Analyze materially different products, geographies, or segments separately instead of averaging incompatible economics. Use seven evidence-led subsections in this order:
1. Offering and job-to-be-done: identify each material product/service, the problem solved, users, customers, and payers, the purchase decision-maker, purchase frequency, alternatives, and the concrete reason customers choose this company. Test whether usage is necessary, habitual, subsidized, or easily substitutable.
2. Transaction lifecycle and value chain: trace the transaction lifecycle from demand creation through discovery, purchase, payment, sourcing, inventory ownership, fulfillment, after-sales, refunds, and repeat purchase. Map which participant owns assets, working capital, data, service obligations, fraud, quality, logistics, regulatory, and counterparty risk; show the flow of goods, services, data, and cash and identify bottlenecks.
3. Monetization and profit pool: give the revenue formula for every material stream, pricing or take-rate mechanism, who actually pays, gross-to-contribution profit bridge, variable and fixed cost stack, working capital, capital expenditure, maintenance versus growth spending, and which participant captures the industry's profit pool. Explicitly identify the Profit engine and difficulty of earning durable profits, rate profit difficulty as easy, moderate, or hard, and explain where accounting profit can diverge from economic profit.
4. Segment-level unit economics and scaling: analyze segment-level unit economics for every materially different segment or geography, including acquisition cost, subsidy, retention/repeat behavior, order or customer economics, fulfillment and support cost, contribution margin, fixed-cost absorption, incremental margin, reinvestment need, and saturation. Never infer attractive unit economics from consolidated growth alone; state the exact missing cohort or segment metric.
5. Competitive intensity and bargaining power: name serious direct rivals and substitutes by segment, customer and merchant multi-homing, switching friction, entry barriers, supplier/channel/customer/regulator power, the cheapest credible attack, likely retaliation, and the attacker's required capital, time, capabilities, distribution and cumulative losses; rate competitive intensity as low, medium, or high and competitive position as weak, mixed, or strong. This is not yet a moat verdict.
6. Positive economics and growth loops: state the minimum conditions that must all remain true, causal reinforcing loops, scalability, incremental-economics improvement, reinvestment runway, and what observable evidence proves the loop is becoming self-funding rather than subsidy-funded.
7. Inversion and failure architecture; Multidisciplinary latticework: work backward from economic-profit collapse even if revenue keeps growing. Cover hidden dependencies, bottlenecks, delays, feedback reversals, second-order cascades, regulation, channel or technology shifts, and earliest break signals. Choose only three to five causally relevant lenses and for each use lens -> observed mechanism -> required evidence -> implication; do not name-drop a model.
For every subsection, distinguish verified fact, reasoned inference, and unknown. Use source-backed operating figures and concrete examples wherever available; explicitly name missing evidence instead of filling gaps with generic industry language.
Then perform a focused Moat audit. Product quality, high market share, execution, management, founder talent, marketing, distribution, and temporary technology leadership are outcomes or capabilities, not moat mechanisms, by themselves. For each supported candidate use mechanism -> customer/cost behavior -> competitor constraint -> retained economic profit; test subsidy removal, replication, switching, founder departure, and channel/technology change. Rate strength none/weak/moderate/strong, duration temporary/medium-term/structural, and give the kill condition.
Use every available period in the structured financial series and never invent a missing year. Complete the shared four-row operating audit and six-row decision matrix below without duplicating prior narrative. End with a Company quality conclusion: knowability, business quality, management/capital-allocation quality, great/good/gruesome classification, decisive variables, earliest failure signals, and unresolved evidence. Cite facts near their claims and label inference."#;

const FOCUSED_BUSINESS_MODEL_ANALYSIS_STRUCTURE: &str = r#"For a focused business-model analysis, explain the causal machinery rather than writing a company description. Target 3,500 to 4,500 Simplified Chinese characters or 1,600 to 2,100 English words when evidence supports it. The business-model core must receive at least 75% of the answer; do not shorten it to make room for the shared audit or decision matrix.
1. Offering and job-to-be-done. Analyze each material product or segment separately: problem solved, user/customer/payer, purchase decision, frequency, alternatives, necessity, substitutability, and why this company wins the transaction.
2. Transaction lifecycle and value chain. Trace demand creation, discovery, purchase, payment, sourcing, inventory, fulfillment, after-sales, refunds, and repeat purchase. Map goods/services/data/cash flows, asset ownership, working capital, participant incentives, risk ownership, dependencies, and bottlenecks.
3. Monetization, profit pool, and segment-level unit economics. Give the revenue formulas, pricing/take-rate logic, cost stack, acquisition and subsidy, retention/repeat behavior, contribution margin, fulfillment/support cost, fixed-cost absorption, capital intensity, maintenance versus growth spending, incremental margin, and who captures or transfers value. Separate materially different products/geographies and do not infer unit economics from consolidated growth.
4. Competitive structure and attacker economics. Name direct rivals and substitutes by segment; analyze multi-homing, switching, stakeholder bargaining power, entry barriers, the cheapest credible attack, likely response, and required capital/time/capability/distribution/cumulative loss. Rate competitive intensity, relative position, and durable-profit difficulty separately.
5. Positive case. State the minimum conditions that must all remain true, reinforcing loops, scalability, reinvestment runway, and source-backed evidence that incremental economics improve without permanent subsidy.
6. Inversion and failure architecture. Work backward from economic-profit collapse or permanent capital loss. Ask how the model can fail even while revenue grows; identify subsidy or cheap-capital dependence, channel/regulatory/key-person/counterparty dependencies, bottlenecks, delays, feedback reversals, cascading second-order effects, and the earliest observable break signal for every material path.
7. Multidisciplinary latticework and integrated verdict. Select only three to five causally relevant lenses from microeconomics and industrial organization, accounting and corporate finance, psychology and incentives, systems thinking, game theory, technology and operations, organizational behavior and regulation, and base rates and the outside view. For each lens, state lens -> observed mechanism -> required evidence -> implication. Do not name-drop a model or substitute a checklist for causal reasoning. Distinguish value creation from value transfer, classify the model as fragile, mixed, or robust, and state the decisive assumption, strongest disconfirming evidence, missing metric, and kill condition.
For every section, separate verified fact, inference, and unknown. Use concrete operating figures and examples from the supplied sources; missing segment or cohort disclosure is itself a finding, not a reason to replace analysis with generic prose."#;

const FOCUSED_MOAT_ANALYSIS_STRUCTURE: &str = r#"For a focused moat analysis, causal proof matters more than praise. Target 3,500 to 4,500 Simplified Chinese characters or 1,600 to 2,100 English words when evidence supports it. Reserve at least 75% of the answer for the moat core below; do not compress it to make room for the shared audit or decision matrix.
A moat is a structural mechanism that prevents competitors from eroding durable excess economic returns, not merely a good company attribute. Separate materially different products, customer groups, and geographies instead of assigning one blended company-wide label.
1. Mechanism map. Identify only the three to five plausible mechanisms that could materially protect economic profit. Test brand pricing power, network effects, scale or cost advantage, switching costs, protected intellectual property, exclusive resources, and regulatory licenses. State which expected categories are absent or immaterial.
2. Moat judgment cards. For every candidate mechanism, provide all of: (a) dated observed facts and nearby source citations; (b) mechanism -> customer or cost behavior -> competitor constraint -> retained economic profit; (c) the relevant competitor or outside-view comparison; (d) recurring maintenance cost and whether it is rising; (e) strongest counterevidence, unknown, and alternative explanation; (f) verdict none/weak/moderate/strong and duration temporary/medium-term/structural; (g) why the verdict is not one level higher or lower; (h) confidence low/medium/high; and (i) upgrade, downgrade, and kill conditions. If evidence is missing, say not established instead of filling the card with generic industry prose.
3. Scoring discipline. Do not use a 1-5 score unless the answer first defines observable anchors for 1, 3, and 5 and has enough evidence to locate the company between them. Any scorecard is only an index into the judgment cards, never the analysis itself. Each scored dimension must show its evidence, confidence, and the exact condition for the adjacent higher and lower score. Avoid unsupported half-point precision; use a range or not scoreable when disclosure is weak.
4. False-moat audit and counterfactuals. Treat product quality, high market share, strong execution, efficient operations, management quality, founder talent, marketing, distribution reach, cash, and temporary technology leadership as outcomes or capabilities unless they demonstrably create a hard-to-copy structural constraint. Test subsidy removal, price cuts, elevated marketing, customer and merchant multi-homing, easy switching, founder departure, channel change, technological change, regulation, and a well-funded rival with patient capital.
5. Breach-path dossiers. Rank only the three to five most material breach paths by likelihood times impact; do not produce a one-sentence threat list. Repeat the same explicit field labels for every selected path: Attacker and starting asset; numbered cheapest attack sequence; prerequisites; required time, capital, capabilities, distribution, and cumulative loss; likely company response; economic transmission into retention, take rate, contribution margin, or economic profit; leading indicators and current evidence; probability and confidence; disconfirming condition; missing evidence; and moat-verdict change point. Every dossier must contain every field. Never compress the final path into prose or omit fields because of answer length; include fewer paths instead. Include self-inflicted failure such as ecosystem degradation or capital misallocation when it is more credible than an external attack.
6. Integrated verdict. Separate verified fact, reasoned inference, and unknown. Identify the strongest disconfirming evidence, decisive missing metric, weakest causal link, and which mechanism contributes most to durable economic profit. Conclude with current strength and duration by segment plus the two or three observations that would most change the judgment. Cite facts near their claims."#;

const DURABLE_OWNER_AUDIT: &str = r#"Shared Buffett-Munger operating audit. Render one compact four-row table with columns Evidence, Verdict, Missing evidence; do not repeat the business-model or moat narrative.
1. Knowability gate: classify predictable / partially predictable / not predictably bounded. Name the three to five decisive operating variables and test historical stability, technology/industry change, cyclicality, regulation, management dependence, and data quality. A financial base is necessary but not sufficient. If not predictably bounded, do not produce numerical five- or ten-year earnings ranges; give qualitative scenario architecture, exact blockers, and evidence that would reopen quantification. Partial predictability permits numbers only for bounded variables.
2. Owner economics and reinvestment: reconcile profit to cash; distinguish maintenance capital expenditure from growth capital expenditure. Operating cash flow minus total capex is only a free-cash-flow proxy when that split is missing. Cover owner earnings, owner earnings per diluted share, dilution/share-based compensation, incremental return on invested capital, return earned on retained earnings, capital intensity, reinvestment runway and duration. Classify great, good, or gruesome and name the class-changing condition.
3. Management, culture, incentives, and capital allocation: observable ability, integrity, candor, owner orientation, customer orientation, succession and key-person dependence; incentive design for executives, employees, channels, customers, and suppliers; deployment across internal reinvestment, acquisitions, debt reduction, dividends, buybacks and idle cash. Flag gaming, empire building, conflicts, or selective disclosure. Management is not automatically a moat.
4. Financial resilience and ruin risk: debt maturities, liquidity, refinancing dependence, working-capital reversals, contingent/off-balance-sheet obligations, customer/merchant funds and regulatory liabilities. Name the breaking stress, buffer, and earliest warning."#;

const MANDATORY_INVESTOR_QUESTIONS: &str = r#"End with a clearly labeled Six mandatory answers decision matrix. When answering in Chinese, use the exact heading `## 六个强制回答决策矩阵`; do not add an English ordinal or placeholder to this heading. Rows: Offering/necessity, Profit truth/sustainability, Maintenance cost, Competitor count/threat, Attacker cost/time/win-rate, 5/10-year earnings power. Columns: verified facts, optimistic case, pessimistic case, current verdict, missing evidence. A row may reference earlier sections but cannot be omitted; keep each non-forecast cell to one short sentence.
1. What exactly does the company provide, and why is it non-substitutable? State user/customer/payer and admit easy substitution or multi-homing.
2. Are reported profits economically real and sustainable? Reconcile earnings with operating cash flow, owner-earnings proxy limits, working capital, maintenance capex, stock compensation, subsidies and one-offs.
3. What recurring cost is required to maintain the advantage? Separate maintenance from growth across marketing, discounts, R&D, capex, fulfillment, compliance and ecosystem concessions.
4. How many serious direct competitors and substitutes exist, and how threatening is each? Count meaningful threats, rate low/medium/high, and name cheapest attack and likely response.
5. From a rational attacker's perspective, what would replacement require? Cover required capital, time, capabilities, distribution, data/supply/regulatory access and cumulative loss. Build an order-of-magnitude range from observable anchors; give probability ranges of matching or surpassing over three, five and ten years. Use numbers only with facts, comparables, or labeled assumptions.
6. What is the company's five-to-ten-year earnings power? Apply the Knowability gate first. A financial base is necessary but not sufficient. Only if decisive variables are bounded, calculate numerical company-operating downside, base, and upside cases labeled 经营悲观 / 经营基准 / 经营乐观, never stock-market bear/bull cases. Show key drivers, revenue, margin and free cash flow or owner earnings at years five and ten; use revenue_t = base revenue × (1 + growth)^t and operating profit_t = revenue_t × operating margin. Do the arithmetic only after the knowability gate passes. If it fails, do not produce numerical five- or ten-year earnings ranges; provide qualitative scenario architecture, blockers, monitoring variables and evidence needed to quantify.
Never invent precise replacement costs, probabilities, or earnings. Wide anchored ranges are better than false precision; otherwise stay qualitative."#;

fn is_broad_company_analysis(context: &ConversationContext) -> bool {
    if !is_company_context(context) {
        return false;
    }
    let message = context.user_message.to_ascii_lowercase();
    let focused_dimensions = [
        BUSINESS_MODEL_TERMS,
        &["财报", "业绩", "收入", "利润", "cash flow", "earnings"],
        &["护城河", "竞争", "moat", "competition"],
        &["估值", "valuation"],
        &["风险", "反证", "risk", "disconfirm"],
    ]
    .iter()
    .filter(|terms| contains_any(&message, terms))
    .count();
    focused_dimensions >= 2 || (focused_dimensions == 0 && requests_generic_analysis(&message))
}

fn requests_generic_analysis(message: &str) -> bool {
    message.starts_with("分析")
        || message.starts_with("研究")
        || message.starts_with("analyze ")
        || message.starts_with("analyse ")
        || message.starts_with("research ")
        || contains_any(
            message,
            &[
                "全面分析",
                "深入分析",
                "详细分析",
                "完整分析",
                "系统分析",
                "company analysis",
                "detailed analysis",
                "comprehensive analysis",
            ],
        )
}

const BUSINESS_MODEL_TERMS: &[&str] = &[
    "商业模式",
    "怎么赚钱",
    "如何赚钱",
    "挣钱",
    "赚钱难",
    "盈利难度",
    "business model",
    "revenue model",
    "value chain",
    "profit pool",
    "profit difficulty",
];

fn is_focused_business_model_analysis(context: &ConversationContext) -> bool {
    is_company_context(context)
        && contains_any(
            &context.user_message.to_ascii_lowercase(),
            &[
                BUSINESS_MODEL_TERMS,
                &[
                    "议价权",
                    "竞争强度",
                    "competitive intensity",
                    "bargaining power",
                ],
            ]
            .concat(),
        )
}

fn is_focused_moat_analysis(context: &ConversationContext) -> bool {
    is_company_context(context) && is_focused_moat_request(&context.user_message)
}

fn is_focused_moat_request(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    contains_any(
        &normalized,
        &[
            "护城河",
            "竞争壁垒",
            "moat",
            "durable competitive advantage",
        ],
    ) && !contains_any(
        &normalized,
        &[
            "商业模式",
            "财报",
            "年报",
            "季报",
            "近五年",
            "近十年",
            "估值",
            "business model",
            "earnings report",
            "annual report",
            "financial statements",
            "filing",
            "valuation",
        ],
    )
}

fn is_company_context(context: &ConversationContext) -> bool {
    context
        .subject
        .get("kind")
        .and_then(serde_json::Value::as_str)
        == Some("company")
}

fn contains_any(value: &str, candidates: &[&str]) -> bool {
    candidates.iter().any(|candidate| value.contains(candidate))
}

#[cfg(test)]
mod tests {
    use super::{
        is_focused_moat_request, BROAD_COMPANY_ANALYSIS_STRUCTURE,
        FOCUSED_BUSINESS_MODEL_ANALYSIS_STRUCTURE, FOCUSED_MOAT_ANALYSIS_STRUCTURE,
        MANDATORY_INVESTOR_QUESTIONS,
    };

    #[test]
    fn detailed_analysis_reserves_real_business_model_depth() {
        for required in [
            "target 4,000 to 5,000 Simplified Chinese characters",
            "at least 60% of the substantive analysis",
            "Do not compress it to make room for later sections",
            "transaction lifecycle",
            "segment-level unit economics",
        ] {
            assert!(BROAD_COMPANY_ANALYSIS_STRUCTURE.contains(required));
        }
        assert!(!BROAD_COMPANY_ANALYSIS_STRUCTURE
            .contains("at most 3,200 Simplified Chinese characters"));
        assert!(FOCUSED_BUSINESS_MODEL_ANALYSIS_STRUCTURE
            .contains("Target 3,500 to 4,500 Simplified Chinese characters"));
    }

    #[test]
    fn focused_moat_analysis_requires_decision_auditable_depth() {
        for required in [
            "Target 3,500 to 4,500 Simplified Chinese characters",
            "Do not use a 1-5 score",
            "why the verdict is not one level higher or lower",
            "Attacker and starting asset; numbered cheapest attack sequence",
            "transmission into retention, take rate, contribution margin, or economic profit",
            "leading indicators",
            "disconfirming condition",
            "Rank only the three to five most material breach paths",
            "Every dossier must contain every field",
            "Never compress the final path into prose",
        ] {
            assert!(FOCUSED_MOAT_ANALYSIS_STRUCTURE.contains(required));
        }
        assert!(MANDATORY_INVESTOR_QUESTIONS.contains("## 六个强制回答决策矩阵"));
    }

    #[test]
    fn moat_risk_and_adversarial_wording_stays_a_focused_moat_request() {
        assert!(is_focused_moat_request(
            "生成详细的护城河分析。不要只给评分表或一句话风险清单；我要知道竞争者如何一步步攻破。"
        ));
        assert!(is_focused_moat_request("分析护城河、反证和竞争风险"));
        assert!(!is_focused_moat_request("研究近五年财报、商业模式和护城河"));
    }
}
