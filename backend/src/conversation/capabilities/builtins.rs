use serde_json::json;

use crate::ai::runtime::TaskComplexity;

use super::{
    manifest::{
        parse_capability_manifest, CapabilityDefinition, CapabilityManifest, CapabilityReference,
    },
    CapabilityContextKey, CapabilityKind, CapabilitySubjectKind, CapabilitySurface,
};

mod schema;
use schema::{
    business_model_analysis_schema, company_analysis_schema, moat_audit_schema,
    thesis_challenge_schema,
};

pub(super) const BUSINESS_MODEL_SKILL: &str = "analyze_business_model";
pub(super) const MOAT_AUDIT_SKILL: &str = "audit_moat";
pub(super) const COMPANY_ANALYSIS_AGENT: &str = "analyze_company";
pub(super) const THESIS_CHALLENGE_AGENT: &str = "challenge_company_thesis";

pub(super) fn builtin_capabilities() -> Vec<CapabilityDefinition> {
    vec![
        business_model(),
        moat_audit(),
        company_analysis(),
        thesis_challenge(),
    ]
}

fn business_model() -> CapabilityDefinition {
    definition(CapabilityManifest {
        id: BUSINESS_MODEL_SKILL.to_string(),
        version: 1,
        kind: CapabilityKind::Skill,
        stage: super::CapabilityStage::Analysis,
        display_name: "商业模式分析".to_string(),
        description: "以价值链、资金流、单位经济和竞争者经济分析公司如何持续赚钱".to_string(),
        artifact_type: "business_model_analysis".to_string(),
        instructions: r#"
Purpose: explain how the company creates value, converts it into durable owner earnings, and could lose that ability. Analyze the company itself, never its share price, market sentiment, or trading setup.

Evidence gate:
- Set evidence_assessment before forming conclusions. A finding is a fact only when its evidence contains an exact supplied source URL that directly supports it. Otherwise label it inference or hypothesis and lower confidence.
- Never invent market share, margins, customer behavior, attacker cost, probability, or five-to-ten-year earnings. Quantify only when supplied evidence or an explicit base-rate calculation supports the range; otherwise state the missing inputs.
- Use community material only as an unverified signal to investigate, never as proof.

Coverage follows evidence quality:
- If evidence is insufficient, do not simulate a complete company analysis. Return a concise summary, critical gaps, open questions, and at most two low-confidence hypotheses that define the next decisive test.
- If evidence is partial, create findings only for supported categories or decision-critical inferences; place uncovered categories in critical_gaps instead of manufacturing filler.
- Only when evidence is sufficient should the result cover every material category below. Separate materially different products, regions, customer groups, and fulfillment models rather than averaging incompatible economics.
1. offering_and_customer: product or service, user, payer, customer job, alternatives, and why replacement is difficult or easy.
2. value_and_cash_flow: the complete product, data, service, risk, and cash path through every important participant.
3. profit_pool_and_costs: revenue mechanics, gross-profit pools, fixed and variable costs, working capital, and who bears inventory, credit, fulfillment, regulatory, and technology risk.
4. unit_economics_and_capital_intensity: acquisition, retention, marginal economics, maintenance versus growth investment, operating leverage, and bottlenecks.
5. owner_economics: reconcile reported profit with cash conversion, maintenance capital, dilution, subsidies, and reinvestment returns.
6. competitive_intensity: named competitors and substitutes, bargaining power, multi-homing, switching behavior, and how hard each profit pool is to earn.
7. attacker_economics: cheapest credible attack sequence, required capital/time/capabilities, incumbent response, cumulative losses, and probability direction. Give a numeric probability only with a cited basis.
8. five_to_ten_year_scenarios: company-operating downside, base, and upside causal paths. Quantify earnings only after the evidence gate passes.

Use only multidisciplinary lenses that change a conclusion. Prefer explicit causal chains, strongest counterarguments, observable leading indicators, and falsification thresholds over labels, scores, or generic checklists.
"#
        .trim()
        .to_string(),
        input_schema: focus_schema(),
        output_schema: business_model_analysis_schema(),
        context: company_analysis_context(),
        model: TaskComplexity::Deep,
        timeout_seconds: 600,
        max_steps: 1,
        tools: Vec::new(),
        skills: Vec::new(),
        surfaces: vec![CapabilitySurface::Conversation],
        subjects: vec![CapabilitySubjectKind::Company],
        triggers: vec![
            "商业模式".to_string(),
            "如何赚钱".to_string(),
            "挣钱难度".to_string(),
            "单位经济".to_string(),
            "business model".to_string(),
            "unit economics".to_string(),
        ],
        initial_activity: "skill_analyzing_business_model".to_string(),
    })
}

fn moat_audit() -> CapabilityDefinition {
    definition(CapabilityManifest {
        id: MOAT_AUDIT_SKILL.to_string(),
        version: 1,
        kind: CapabilityKind::Skill,
        stage: super::CapabilityStage::Analysis,
        display_name: "护城河审计".to_string(),
        description: "验证竞争优势能否限制竞争者并保留长期经济利润".to_string(),
        artifact_type: "moat_audit".to_string(),
        instructions: r#"
Purpose: determine whether a structural mechanism prevents competitors from eroding the company's economic profit. Product quality, market share, management, execution, marketing, distribution, scale, and temporary technical leadership are evidence candidates, not moats by themselves.

Evidence gate:
- Set evidence_assessment first. A mechanism is verified only when supplied sources support both its operation and its economic consequence. Without that chain, label it inference or hypothesis and lower confidence.
- Do not infer durability from current size, growth, customer satisfaction, or historical profitability alone. Do not assign a score, duration, replication cost, or attacker probability without observable anchors.
- Community viewpoints may identify attack paths but cannot verify a moat.

Coverage follows evidence quality. With insufficient evidence, do not walk every category or pronounce a moat; return gaps and at most two candidate-mechanism hypotheses that direct verification. With partial evidence, include only mechanisms whose causal chain can be materially assessed. With sufficient evidence, cover the material categories below:
1. candidate_mechanism: identify only mechanisms that may survive removal of subsidy, product imitation, management turnover, channel change, and technology change.
2. competitor_constraint: show the full chain from structural mechanism, to customer/cost behavior, to a concrete constraint on named competitors, to retained economic profit.
3. maintenance_cost: identify recurring product, incentive, content, distribution, compliance, capital, and organizational costs required to preserve the mechanism.
4. attacker_breach_path: give the cheapest credible sequence for a well-funded competitor, prerequisites, cumulative losses, company response, economic transmission, and earliest warning signal.
5. durability_and_failure: state expected direction and duration only when evidence supports them; otherwise identify what must be observed. Define downgrade and failure conditions precisely.

Prefer two or three complete mechanisms over many shallow labels. Every finding must include the strongest counterargument, unresolved evidence, leading indicators, falsification condition, and the exact decision consequence if the mechanism strengthens or fails.
"#
        .trim()
        .to_string(),
        input_schema: focus_schema(),
        output_schema: moat_audit_schema(),
        context: company_analysis_context(),
        model: TaskComplexity::Deep,
        timeout_seconds: 600,
        max_steps: 1,
        tools: Vec::new(),
        skills: Vec::new(),
        surfaces: vec![CapabilitySurface::Conversation],
        subjects: vec![CapabilitySubjectKind::Company],
        triggers: vec![
            "护城河".to_string(),
            "竞争优势".to_string(),
            "竞争壁垒".to_string(),
            "moat".to_string(),
            "competitive advantage".to_string(),
        ],
        initial_activity: "skill_auditing_moat".to_string(),
    })
}

fn thesis_challenge() -> CapabilityDefinition {
    definition(CapabilityManifest {
        id: THESIS_CHALLENGE_AGENT.to_string(),
        version: 1,
        kind: CapabilityKind::Agent,
        stage: super::CapabilityStage::Challenge,
        display_name: "反方分析".to_string(),
        description: "从相同证据快照独立寻找足以推翻当前公司判断的机制和证据".to_string(),
        artifact_type: "thesis_challenge".to_string(),
        instructions: r#"
Role: act as an independent dissent analyst. Steelman the company thesis before challenging it. Do not mechanically negate the user, use share-price arguments, list generic risks, or treat uncertainty as proof of failure.

Evidence gate:
- Set evidence_assessment first. Use exact supplied source URLs for facts; mark unsupported mechanisms as inference or hypothesis. Community viewpoints are leads, not proof.
- Request more research only when one missing fact could materially change a failure-mode ranking. After an unavailable source, do not repeat the same research path with cosmetic wording.
- Do not assign numeric probabilities or losses without cited evidence or an explicit base rate.

Required analysis:
1. Identify the minimum set of thesis_assumption findings on which the positive case actually depends.
2. When evidence is sufficient or partial, select only three to five material failure modes from operating, accounting, competitive, incentive, regulatory, and capital-allocation categories. Do not manufacture one from every category. When evidence or the positive thesis is insufficient, return a concise abstention and at most two hypotheses describing what must be tested before a dissent case can be ranked.
3. For each failure mode provide the causal sequence, supplied evidence, missing evidence, strongest rebuttal, probability direction, impact transmission, earliest observable indicator, falsification condition, and the exact thesis conclusion that would change.
4. Before submitting the final object, remove duplicated mechanisms, generic language, unsupported precision, and any argument that does not engage the strongest positive rebuttal. This is a completeness check on the current result, not a reference to an unavailable prior draft.

The final output should help the user know what to monitor and what would genuinely invalidate the company view, not merely feel more cautious.
"#
        .trim()
        .to_string(),
        input_schema: focus_schema(),
        output_schema: thesis_challenge_schema(),
        context: company_analysis_context(),
        model: TaskComplexity::Deep,
        timeout_seconds: 600,
        max_steps: 6,
        tools: vec![
            capability_ref(super::RESEARCH_COMPANY_TOOL),
            capability_ref(super::RESEARCH_COMMUNITY_INSIGHTS_TOOL),
        ],
        skills: vec![
            capability_ref(BUSINESS_MODEL_SKILL),
            capability_ref(MOAT_AUDIT_SKILL),
        ],
        surfaces: vec![CapabilitySurface::Conversation],
        subjects: vec![CapabilitySubjectKind::Company],
        triggers: vec![
            "反方分析".to_string(),
            "反对观点".to_string(),
            "挑战我的观点".to_string(),
            "反驳投资逻辑".to_string(),
            "证伪投资逻辑".to_string(),
            "dissent".to_string(),
            "challenge the thesis".to_string(),
        ],
        initial_activity: "agent_challenging_company_thesis".to_string(),
    })
}

fn company_analysis() -> CapabilityDefinition {
    definition(CapabilityManifest {
        id: COMPANY_ANALYSIS_AGENT.to_string(),
        version: 1,
        kind: CapabilityKind::Agent,
        stage: super::CapabilityStage::Analysis,
        display_name: "公司深度分析".to_string(),
        description: "协调商业模式、护城河和证据检索，形成可证伪的公司整体判断".to_string(),
        artifact_type: "company_analysis".to_string(),
        instructions: r#"
Role: act as the lead company analyst and produce a decision-useful, falsifiable view of the company itself. Never discuss share price, technical signals, target prices, or trading sentiment. Apply the loaded business-model and moat skills as methods, not as evidence or prewritten conclusions.

Evidence gate and research policy:
- Inspect the frozen evidence before requesting research. Set evidence_assessment from the evidence actually available, including its recency and decisive gaps.
- Call one read-only research tool only when a specific missing fact or contradiction could change a material conclusion. State that gap in focus; do not issue broad or cosmetically different repeat calls. If retrieval remains unavailable, finish with an insufficient or partial assessment instead of filling gaps from general knowledge.
- A fact requires an exact supplied source URL. Separate primary facts, independent interpretation, and unverified community signals. Never promote a Skill result or community opinion into evidence.
- Do not invent market share, margins, owner earnings, probabilities, attacker costs, or forecasts. Quantify only from supplied evidence or explicit, inspectable assumptions.

Coverage follows evidence quality:
- If evidence is insufficient, do not simulate a complete company analysis. Return a concise summary, critical gaps, open questions, and at most two low-confidence hypotheses that define the next decisive test.
- If evidence is partial, create findings only for supported categories or decision-critical inferences; place uncovered categories in critical_gaps instead of manufacturing filler.
- Only when evidence is sufficient should the result cover every material category below.
1. business_model: products, users, payers, value/cash flow, profit pools, segment differences, unit economics, and risk ownership.
2. owner_economics: cash conversion, maintenance investment, dilution, reinvestment runway, and incremental return.
3. competitive_position: named competitors and substitutes, bargaining power, switching/multi-homing, and profit-pool difficulty.
4. moat: verified mechanisms, competitor constraints, maintenance cost, attacker path, durability, and failure conditions.
5. management_and_capital_allocation: incentives, operating culture, reinvestment, acquisitions, distributions, and evidence of discipline.
6. financial_resilience: balance-sheet obligations, cyclicality, liquidity, hidden claims, and permanent-loss pathways.
7. earning_power: company-operating downside, base, and upside paths over five to ten years; use qualitative scenarios when the evidence gate blocks numbers.
8. failure_mechanism: the strongest causal path that could invalidate the overall view and its earliest observable signal.

For every finding include evidence, causal chain, strongest counterargument, unknowns, confidence, leading indicators, falsification, and decision impact. Prefer a compact complete causal model over repeated prose or generic risk lists.
"#
        .trim()
        .to_string(),
        input_schema: focus_schema(),
        output_schema: company_analysis_schema(),
        context: company_analysis_context(),
        model: TaskComplexity::Deep,
        timeout_seconds: 600,
        max_steps: 8,
        tools: vec![
            capability_ref(super::RESEARCH_COMPANY_TOOL),
            capability_ref(super::RESEARCH_COMMUNITY_INSIGHTS_TOOL),
        ],
        skills: vec![
            capability_ref(BUSINESS_MODEL_SKILL),
            capability_ref(MOAT_AUDIT_SKILL),
        ],
        surfaces: vec![CapabilitySurface::Conversation],
        subjects: vec![CapabilitySubjectKind::Company],
        triggers: vec![
            "公司分析".to_string(),
            "全面分析".to_string(),
            "深度分析".to_string(),
            "完整分析".to_string(),
            "company analysis".to_string(),
            "company deep dive".to_string(),
            "analyze the company".to_string(),
        ],
        initial_activity: "agent_analyzing_company".to_string(),
    })
}

fn capability_ref(id: &str) -> CapabilityReference {
    CapabilityReference {
        id: id.to_string(),
        version: 1,
    }
}

fn company_analysis_context() -> Vec<CapabilityContextKey> {
    vec![
        CapabilityContextKey::Subject,
        CapabilityContextKey::UserMessage,
        CapabilityContextKey::CompanyView,
        CapabilityContextKey::ResearchSources,
        CapabilityContextKey::Attachments,
    ]
}

fn focus_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["focus"],
        "properties": {
            "focus": { "type": "string", "maxLength": 4000 }
        }
    })
}

fn definition(manifest: CapabilityManifest) -> CapabilityDefinition {
    let content = serde_json::to_string(&manifest).expect("built-in capability serializes");
    parse_capability_manifest(&content).expect("built-in capability manifest is valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_analysis_prompts_abstain_when_evidence_is_insufficient() {
        for capability in builtin_capabilities() {
            assert!(
                capability.manifest.instructions.contains("at most two"),
                "{} must bound insufficient-evidence hypotheses",
                capability.manifest.id
            );
        }
        assert!(!company_analysis()
            .manifest
            .instructions
            .contains("return one material finding for every output category"));
    }
}
