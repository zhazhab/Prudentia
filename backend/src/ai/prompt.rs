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
        serde_json::to_string_pretty(context).expect("research context serializes")
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
        serde_json::to_string_pretty(context).expect("research context serializes")
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portfolio::PortfolioSummary;

    #[test]
    fn stock_snapshot_prompt_contains_trading_instruction_guardrail() {
        let prompt = stock_snapshot_prompt(
            &StockSnapshotContext {
                symbol: "AAPL".to_string(),
                position: None,
                portfolio_summary: empty_portfolio_summary(),
                related_memos: Vec::new(),
                selected_memo: None,
                quote: None,
                quote_error: None,
            },
            Locale::En,
        );

        assert!(prompt.contains("Do not give buy, sell, trim, add, or hold instructions"));
    }

    #[test]
    fn portfolio_review_prompt_contains_trading_instruction_guardrail() {
        let prompt = portfolio_review_prompt(
            &PortfolioReviewContext {
                positions: Vec::new(),
                summary: empty_portfolio_summary(),
                holdings_without_memo: Vec::new(),
            },
            Locale::En,
        );

        assert!(prompt.contains("Do not give buy, sell, trim, add, or hold instructions"));
    }

    #[test]
    fn research_distillation_prompt_contains_external_fact_guardrail() {
        let prompt = research_distillation_prompt(
            &ResearchSourceInput {
                title: "Munger notes".to_string(),
                source_type: Some("person".to_string()),
                source_title: Some("Interview notes".to_string()),
                source_author: Some("Charlie Munger".to_string()),
                source_content: "Invert before deciding.".to_string(),
                symbol: None,
            },
            Locale::En,
        );

        assert!(prompt.contains("Do not invent external facts"));
    }

    fn empty_portfolio_summary() -> PortfolioSummary {
        PortfolioSummary {
            total_market_value: 0.0,
            total_cost: 0.0,
            total_unrealized_pnl: 0.0,
            positions_count: 0,
            price_stale_count: 0,
            top_positions: Vec::new(),
            sectors: Vec::new(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }
}
