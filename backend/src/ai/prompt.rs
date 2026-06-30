use crate::{
    ai::{PortfolioReviewContext, ResearchSourceInput, StockSnapshotContext},
    investment_system::InvestmentSystem,
    locale::Locale,
    memo::Memo,
};

pub fn memo_extraction_prompt(memo: &Memo, locale: Locale) -> String {
    format!(
        r#"
Return strict JSON only, with no markdown fences. The JSON shape is:
{{
  "thesis": "string",
  "risks": "string",
  "catalysts": "string",
  "disconfirming_evidence": "string",
  "checklist": ["string"]
}}

Language: {}

Extract an investment memo from this draft:
Title: {}
Symbol: {:?}
Notes:
{}
"#,
        language_name(locale),
        memo.title,
        memo.symbol,
        memo.notes
    )
}

pub fn investment_system_refinement_prompt(system: &InvestmentSystem, locale: Locale) -> String {
    format!(
        r#"
Return strict JSON only, with no markdown fences. The JSON shape is:
{{
  "principles": ["string"],
  "checklist_items": ["string"],
  "circle_of_competence": ["string"],
  "decision_rules": ["string"],
  "summary": "string"
}}

Language: {}

Refine this personal investment system:
{}
"#,
        language_name(locale),
        serde_json::to_string_pretty(system).unwrap_or_default()
    )
}

pub fn research_distillation_prompt(input: &ResearchSourceInput, locale: Locale) -> String {
    format!(
        r#"
Return strict JSON only, with no markdown fences. The JSON shape is:
{{
  "summary": "string",
  "insights": ["string"],
  "risks": ["string"],
  "checklist": ["string"],
  "candidate_principles": ["string"],
  "candidate_checklist_items": ["string"]
}}

Language: {}

Distill the research source below into investment-research notes. Do not invent external facts.
Title: {}
Source type: {:?}
Source title: {:?}
Source author: {:?}
Symbol: {:?}
Source content:
{}
"#,
        language_name(locale),
        input.title,
        input.source_type,
        input.source_title,
        input.source_author,
        input.symbol,
        input.source_content
    )
}

pub fn stock_snapshot_prompt(context: &StockSnapshotContext, locale: Locale) -> String {
    format!(
        r#"
Return strict JSON only, with no markdown fences. The JSON shape is:
{{
  "summary": "string",
  "insights": ["string"],
  "risks": ["string"],
  "checklist": ["string"],
  "candidate_principles": ["string"],
  "candidate_checklist_items": ["string"]
}}

Language: {}

Analyze this stock snapshot context for research purposes. Do not give buy, sell, trim, add, or hold instructions.
Context:
{}
"#,
        language_name(locale),
        serde_json::to_string_pretty(context).unwrap_or_default()
    )
}

pub fn portfolio_review_prompt(context: &PortfolioReviewContext, locale: Locale) -> String {
    format!(
        r#"
Return strict JSON only, with no markdown fences. The JSON shape is:
{{
  "summary": "string",
  "insights": ["string"],
  "risks": ["string"],
  "checklist": ["string"],
  "candidate_principles": ["string"],
  "candidate_checklist_items": ["string"]
}}

Language: {}

Review this portfolio risk context for research purposes. Do not give buy, sell, trim, add, or hold instructions.
Context:
{}
"#,
        language_name(locale),
        serde_json::to_string_pretty(context).unwrap_or_default()
    )
}

pub fn extract_json_object(value: &str) -> Option<&str> {
    let trimmed = value
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    trimmed.get(start..=end)
}

fn language_name(locale: Locale) -> &'static str {
    if locale.is_zh() {
        "Simplified Chinese"
    } else {
        "English"
    }
}
