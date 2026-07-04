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

pub fn portfolio_image_recognition_prompt() -> String {
    r#"
Return strict JSON only, with no markdown fences.

Extract only the visible portfolio holding rows from the attached screenshot.
Skip pure cash, balance, buying power, fund balance, totals, summary rows, and hidden rows.
Keep ETF/fund/security rows even when their visible name contains "cash" or "现金" if the row has holding-level quantity, cost/current price, market value, or P/L.
Rows inside visible holdings, positions, assets, or securities tables are holding candidates. Do not stop after the first few rows; scan the entire visible table.
When a row appears inside the visible holdings table and has holding metrics, treat it as a holding candidate even if its name looks like cash or balance wording.

Field rules:
- Use only visible row data for names, quantities, average costs, current/last prices, market values, and notes.
- If a security code/ticker is not visible, leave "symbol" empty. Do not invent codes.
- "currency" must be one of CNY, HKD, USD, or an empty string.
- "market" must be one of CN, HK, US, Other, or null.
- Strip currency symbols and thousands separators from numeric fields. Example: HK$489.877 -> 489.877.
- If a current price, last price, or 现价 is visible, put it in "last_price"; do not put cost basis there.
- If a row shows HK$, 港股, 港股通, 沪港, or 深港, use currency HKD and market HK.
- If a row is under an A股 tab or appears to be an A-share/ETF row with no currency symbol, use currency CNY and market CN.
- Do not warn just because optional fields such as sector/account are not visible.
- Only include warnings for genuinely ambiguous or low-confidence rows, and keep warnings short.
If a field is not visible and cannot be inferred from visible market/currency context, use an empty string for required string fields and null for optional fields.

Return this JSON shape:
{
  "rows": [
    {
      "symbol": "string",
      "name": "string",
      "quantity": "string",
      "average_cost": "string",
      "currency": "string",
      "account": "string or null",
      "market": "string or null",
      "sector": "string or null",
      "imported_market_value": "string or null",
      "last_price": "string or null",
      "notes": "string or null",
      "confidence": "high|medium|low|unknown",
      "warnings": ["string"]
    }
  ],
  "warnings": ["string"]
}
"#
    .trim()
    .to_string()
}

pub fn portfolio_image_recognition_schema() -> &'static str {
    r#"
{
  "type": "object",
  "additionalProperties": false,
  "properties": {
    "rows": {
      "type": "array",
      "items": {
        "type": "object",
        "additionalProperties": false,
        "properties": {
          "symbol": { "type": "string" },
          "name": { "type": "string" },
          "quantity": { "type": "string" },
          "average_cost": { "type": "string" },
          "currency": { "type": "string" },
          "account": { "type": ["string", "null"] },
          "market": { "type": ["string", "null"] },
          "sector": { "type": ["string", "null"] },
          "imported_market_value": { "type": ["string", "null"] },
          "last_price": { "type": ["string", "null"] },
          "notes": { "type": ["string", "null"] },
          "confidence": {
            "type": "string",
            "enum": ["high", "medium", "low", "unknown"]
          },
          "warnings": {
            "type": "array",
            "items": { "type": "string" }
          }
        },
        "required": [
          "symbol",
          "name",
          "quantity",
          "average_cost",
          "currency",
          "account",
          "market",
          "sector",
          "imported_market_value",
          "last_price",
          "notes",
          "confidence",
          "warnings"
        ]
      }
    },
    "warnings": {
      "type": "array",
      "items": { "type": "string" }
    }
  },
  "required": ["rows", "warnings"]
}
"#
    .trim()
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

    #[test]
    fn portfolio_image_prompt_keeps_cash_named_etf_rows() {
        let prompt = portfolio_image_recognition_prompt();

        assert!(prompt.contains("holding-level quantity"));
        assert!(prompt.contains("holding candidate"));
        assert!(prompt.contains("Do not stop after the first few rows"));
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
            market_groups: Vec::new(),
            base_currency: "CNY".to_string(),
            total_market_value_base: 0.0,
            total_cost_base: 0.0,
            total_unrealized_pnl_base: 0.0,
            fx_rates: Vec::new(),
            fx_stale_count: 0,
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }
}
