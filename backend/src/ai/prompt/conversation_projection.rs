use crate::{ai::ConversationContext, locale::Locale};

const OBJECT_OUTPUT_CONTRACT: &str = r#"{
  "summary": "short immutable summary of this turn",
  "actions": [
    {
      "action_type": "company_view_patch|trade_record|rule_graph_patch",
      "title": "short title",
      "rationale": "why this durable change follows from the discussion",
      "payload": {}
    }
  ]
}"#;

const CLI_OUTPUT_CONTRACT: &str = r#"{
  "summary": "short immutable summary of this turn",
  "actions": [
    {
      "action_type": "company_view_patch|trade_record|rule_graph_patch",
      "title": "short title",
      "rationale": "why this durable change follows from the discussion",
      "payload": "one valid JSON object encoded as a string"
    }
  ]
}"#;

const MOAT_PROJECTION_FIDELITY: &str = r#"For a focused moat turn, do not collapse the analysis into a single ranked-path paragraph. Preserve every material judgment card and every selected breach path. Use compact Markdown headings or bullets, but repeat these fields for each path: attacker/start asset, attack sequence, prerequisites and resource burden, company response, economic transmission, leading indicators/current evidence, probability/confidence, disconfirming condition, missing evidence, and verdict-change point. Drop repeated exposition before dropping any of these decision fields."#;

const COMPANY_VIEW_SECTION_BUDGETS: &str = r#"Company-view storage budgets are deliberate upper bounds: moat 3,500 characters, business_quality 3,000, financials 2,000, every other changed section 1,200, and all changed sections combined 9,000. Use the available budget when decision-relevant evidence requires it. Compress repeated narrative and methodology first; never remove sources, counterevidence, uncertainty, leading indicators, or verdict-change conditions merely to make a section short."#;

pub fn conversation_projection_prompt(
    context: &ConversationContext,
    assistant_response: &str,
    locale: Locale,
) -> String {
    build_prompt(
        context,
        assistant_response,
        locale,
        OBJECT_OUTPUT_CONTRACT,
        "Each action payload must be a JSON object.",
    )
}

pub fn conversation_projection_cli_prompt(
    context: &ConversationContext,
    assistant_response: &str,
    locale: Locale,
) -> String {
    build_prompt(
        context,
        assistant_response,
        locale,
        CLI_OUTPUT_CONTRACT,
        "Each action payload must be one valid JSON object serialized into the payload string. All payload rules below apply to the decoded object. Use a compact JSON string, not Markdown fences.",
    )
}

fn build_prompt(
    context: &ConversationContext,
    assistant_response: &str,
    locale: Locale,
    output_contract: &str,
    payload_instruction: &str,
) -> String {
    format!(
        r#"
Return strict JSON only with this shape:
{output_contract}

{payload_instruction}

Language: {language}

Create no action for greetings, generic questions, or tentative ideas without a material new conclusion.
Create a company_view_patch whenever the discussion reaches a material new or corrected company conclusion. Its payload must contain symbol, company_name, and changes. changes may contain only business_quality, moat, financials, valuation_expectations, thesis, risks, catalysts, disconfirming_evidence, and open_questions. Treat business_quality as "Business Model, Competition, and Profit Quality": users, customers, payers, value chain, monetization, competitive intensity, relative competitive position, bargaining power, profit pool, pricing power, cost and reinvestment structure, unit economics, capital intensity, and the difficulty of earning durable profits. Preserve the positive case, inversion/failure paths, causally relevant multidisciplinary lenses, robustness verdict, evidence, earliest break signals, and kill condition whenever materially established. Distinguish current competition from moat durability. Include this field whenever the turn establishes or corrects a material business-model conclusion. Use a Markdown string for every section except open_questions, which must be a string array; do not emit atomic claims.
Treat moat as a structural mechanism that protects durable excess economic returns. The moat field must preserve each material mechanism's causal chain, segment boundary, strength, durability, supporting and disconfirming evidence, maintenance cost, confidence, adjacent-verdict conditions, false-moat exclusions, and upgrade/downgrade/kill conditions. Preserve ranked breach paths with the attacker, cheapest sequence, required time/capital/capabilities, likely company response, economic-profit transmission, leading indicators, probability/confidence, disconfirming condition, and missing evidence. Never record product quality, market share, management, execution, marketing, distribution, founder talent, or temporary technology leadership as a moat without evidence that it creates a hard-to-copy competitor constraint. Do not preserve an unsupported numerical score as if it were a fact; retain the rubric and uncertainty or mark it not scoreable.
{moat_projection_fidelity}
For company-analysis projections, business_quality must preserve the offering, evidence of non-substitutability, profit authenticity and sustainability, maintenance cost, knowability classification, decisive operating variables, management ability, integrity, candor, owner orientation, succession risk, incentive design and capital-allocation record. Moat must preserve the serious competitor map, threat ratings, and attacker replacement economics with assumptions. Financials must preserve the normalized earnings base, owner earnings and owner earnings per diluted share, maintenance-versus-growth-capex uncertainty, dilution and share-based compensation, incremental return on invested capital, return on retained earnings, reinvestment runway, great/good/gruesome classification, financial resilience, confidence, and missing evidence. Preserve numerical five-to-ten-year company-operating downside/base/upside earnings ranges only when the knowability gate passes. When it does not pass, store the qualitative scenario architecture and blockers instead of manufacturing numbers. These are company performance scenarios, not stock-market bear/bull, share-price, or valuation-multiple scenarios. When the response contains supported numerical attacker cost/time/probability or earnings ranges, copy those ranges and their key assumptions faithfully instead of weakening them to words such as "large," "multi-year," or "uncertain"; prioritize decision quantities over repeated narrative when fitting the section limit. Do not convert unsupported qualitative judgments into precise numbers.
For a company_view_patch, omit valuation_expectations under the current company-only scope. If thesis is present, it means the operating company thesis, never a security thesis. Exclude share prices, quotations, market capitalization, valuation multiples, price targets, stock returns, technical analysis, analyst ratings, portfolio exposure or profit/loss, and buy/sell/hold implications from the summary, rationale, and every company-view section.
Keep the summary under 120 characters, each title under 40 characters, and each rationale under 160 characters. Include only sections materially supported by this turn.
{company_view_section_budgets}
Create a trade_record only when the user states an actual completed buy or sell and all required fields are known. Its payload must contain side, symbol, quantity, price, currency, occurred_at; fees, account, notes, and corrects_trade_id are optional. Never turn a hypothetical trade into a record.
Create a rule_graph_patch only when the user explicitly confirms that a rule should be added or changed. Its payload must contain base_version and a complete graph with graph_id, name, nodes, and edges. Nodes must be fixed, skill, or agent and have typed configuration; do not encode executable conditions as vague prose.
Each action is independent. Do not claim that it has already executed.

Projection context:
{projection_context}

Visible assistant response:
{assistant_response}
"#,
        language = super::language_name(locale),
        moat_projection_fidelity = MOAT_PROJECTION_FIDELITY,
        company_view_section_budgets = COMPANY_VIEW_SECTION_BUDGETS,
        projection_context =
            serde_json::to_string(&super::conversation_context::projection(context))
                .expect("conversation projection context serializes"),
    )
}

pub fn conversation_projection_schema() -> &'static str {
    r#"{
  "type": "object",
  "additionalProperties": false,
  "properties": {
    "summary": { "type": "string" },
    "actions": {
      "type": "array",
      "items": {
        "type": "object",
        "additionalProperties": false,
        "properties": {
          "action_type": {
            "type": "string",
            "enum": ["company_view_patch", "trade_record", "rule_graph_patch"]
          },
          "title": { "type": "string" },
          "rationale": { "type": "string" },
          "payload": { "type": "string" }
        },
        "required": ["action_type", "title", "rationale", "payload"]
      }
    }
  },
  "required": ["summary", "actions"]
}"#
    .trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_schema_is_valid_and_requires_the_envelope() {
        let schema: serde_json::Value =
            serde_json::from_str(conversation_projection_schema()).expect("valid schema JSON");

        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(
            schema["required"],
            serde_json::json!(["summary", "actions"])
        );
        assert_eq!(
            schema["properties"]["actions"]["items"]["properties"]["payload"]["type"],
            "string"
        );
    }

    #[test]
    fn focused_moat_projection_preserves_decision_fields_with_bounded_storage() {
        for required in [
            "do not collapse the analysis into a single ranked-path paragraph",
            "repeat these fields for each path",
            "leading indicators/current evidence",
            "missing evidence",
            "verdict-change point",
        ] {
            assert!(MOAT_PROJECTION_FIDELITY.contains(required));
        }
        for required in [
            "moat 3,500 characters",
            "business_quality 3,000",
            "all changed sections combined 9,000",
            "Compress repeated narrative and methodology first",
        ] {
            assert!(COMPANY_VIEW_SECTION_BUDGETS.contains(required));
        }
        assert!(!COMPANY_VIEW_SECTION_BUDGETS
            .contains("each changed company-view section under 500 characters"));
    }
}
