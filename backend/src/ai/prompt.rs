use crate::{investment_system::InvestmentSystem, locale::Locale, memo::Memo};

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
