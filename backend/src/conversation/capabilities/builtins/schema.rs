use serde_json::{json, Value};

pub(super) fn business_model_analysis_schema() -> Value {
    structured_analysis_schema(&[
        "offering_and_customer",
        "value_and_cash_flow",
        "profit_pool_and_costs",
        "unit_economics_and_capital_intensity",
        "owner_economics",
        "competitive_intensity",
        "attacker_economics",
        "five_to_ten_year_scenarios",
    ])
}

pub(super) fn moat_audit_schema() -> Value {
    structured_analysis_schema(&[
        "candidate_mechanism",
        "competitor_constraint",
        "maintenance_cost",
        "attacker_breach_path",
        "durability_and_failure",
    ])
}

pub(super) fn company_analysis_schema() -> Value {
    structured_analysis_schema(&[
        "business_model",
        "owner_economics",
        "competitive_position",
        "moat",
        "management_and_capital_allocation",
        "financial_resilience",
        "earning_power",
        "failure_mechanism",
    ])
}

pub(super) fn thesis_challenge_schema() -> Value {
    structured_analysis_schema(&[
        "thesis_assumption",
        "operating_failure",
        "accounting_failure",
        "competitive_failure",
        "incentive_failure",
        "regulatory_failure",
        "capital_allocation_failure",
    ])
}

fn structured_analysis_schema(categories: &[&str]) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["evidence_assessment", "summary", "findings", "open_questions"],
        "properties": {
            "evidence_assessment": evidence_assessment_schema(),
            "summary": { "type": "string", "maxLength": 6000 },
            "findings": {
                "type": "array",
                "maxItems": 12,
                "items": finding_schema(categories)
            },
            "open_questions": {
                "type": "array",
                "maxItems": 12,
                "items": { "type": "string", "maxLength": 800 }
            }
        }
    })
}

fn evidence_assessment_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["status", "rationale", "latest_evidence_date", "critical_gaps"],
        "properties": {
            "status": {
                "type": "string",
                "enum": ["sufficient", "partial", "insufficient"]
            },
            "rationale": { "type": "string", "maxLength": 1800 },
            "latest_evidence_date": { "type": "string", "maxLength": 80 },
            "critical_gaps": {
                "type": "array",
                "maxItems": 10,
                "items": { "type": "string", "maxLength": 600 }
            }
        }
    })
}

fn finding_schema(categories: &[&str]) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": [
            "category",
            "title",
            "judgment",
            "claim_type",
            "evidence",
            "causal_chain",
            "counterargument",
            "unknowns",
            "confidence",
            "leading_indicators",
            "falsification",
            "decision_impact"
        ],
        "properties": {
            "category": {
                "type": "string",
                "enum": categories
            },
            "title": { "type": "string", "maxLength": 240 },
            "judgment": { "type": "string", "maxLength": 3000 },
            "claim_type": {
                "type": "string",
                "enum": ["fact", "inference", "hypothesis"]
            },
            "evidence": {
                "type": "array",
                "maxItems": 10,
                "items": evidence_claim_schema()
            },
            "causal_chain": {
                "type": "array",
                "maxItems": 10,
                "items": { "type": "string", "maxLength": 700 }
            },
            "counterargument": { "type": "string", "maxLength": 2000 },
            "unknowns": {
                "type": "array",
                "maxItems": 8,
                "items": { "type": "string", "maxLength": 600 }
            },
            "confidence": {
                "type": "string",
                "enum": ["low", "medium", "high"]
            },
            "leading_indicators": {
                "type": "array",
                "maxItems": 8,
                "items": { "type": "string", "maxLength": 600 }
            },
            "falsification": { "type": "string", "maxLength": 1200 },
            "decision_impact": { "type": "string", "maxLength": 1500 }
        }
    })
}

fn evidence_claim_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["claim", "source_urls", "as_of"],
        "properties": {
            "claim": { "type": "string", "maxLength": 1200 },
            "source_urls": {
                "type": "array",
                "maxItems": 8,
                "items": { "type": "string", "maxLength": 2000 }
            },
            "as_of": { "type": "string", "maxLength": 80 }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_builtin_schema_has_a_distinct_category_contract() {
        let business_schema = business_model_analysis_schema();
        let moat_schema = moat_audit_schema();
        let company_schema = company_analysis_schema();
        let challenge_schema = thesis_challenge_schema();
        let business = categories(&business_schema);
        let moat = categories(&moat_schema);
        let company = categories(&company_schema);
        let challenge = categories(&challenge_schema);

        assert!(business.contains(&"attacker_economics"));
        assert!(moat.contains(&"attacker_breach_path"));
        assert!(company.contains(&"management_and_capital_allocation"));
        assert!(challenge.contains(&"accounting_failure"));
        assert_ne!(business, moat);
        assert_ne!(company, challenge);
    }

    #[test]
    fn evidence_claims_require_source_urls_and_dates() {
        let schema = company_analysis_schema();
        let required = schema
            .pointer("/properties/findings/items/properties/evidence/items/required")
            .and_then(Value::as_array)
            .expect("evidence required fields");
        assert!(required.contains(&json!("claim")));
        assert!(required.contains(&json!("source_urls")));
        assert!(required.contains(&json!("as_of")));
    }

    fn categories(schema: &Value) -> Vec<&str> {
        schema
            .pointer("/properties/findings/items/properties/category/enum")
            .and_then(Value::as_array)
            .expect("finding categories")
            .iter()
            .filter_map(Value::as_str)
            .collect()
    }
}
