use std::collections::HashSet;

use serde_json::Value;

use super::ToolExecutionError;

pub(super) fn validate_schema_contract(
    schema: &Value,
    label: &str,
) -> Result<(), ToolExecutionError> {
    crate::json_schema::validate_schema_contract(schema, label)
        .map_err(|message| ToolExecutionError::new("invalid_capability_schema", message))
}

pub(super) fn validate_json_schema(
    value: &Value,
    schema: &Value,
    label: &str,
) -> Result<(), ToolExecutionError> {
    crate::json_schema::validate_json_schema(value, schema, label)
        .map_err(|message| ToolExecutionError::new("capability_schema_mismatch", message))
}

pub(super) fn context_source_urls(context: &Value) -> HashSet<String> {
    context
        .get("research_sources")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|source| source.get("url").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

pub(super) fn validate_evidence_urls(
    output: &Value,
    allowed_urls: &HashSet<String>,
) -> Result<(), ToolExecutionError> {
    validate_evidence_at(output, allowed_urls, "capability output")
}

fn validate_evidence_at(
    value: &Value,
    allowed_urls: &HashSet<String>,
    path: &str,
) -> Result<(), ToolExecutionError> {
    match value {
        Value::Object(object) => {
            if object.get("claim_type").and_then(Value::as_str) == Some("fact")
                && !has_evidence_source(object.get("evidence"))
            {
                return Err(ToolExecutionError::new(
                    "capability_schema_mismatch",
                    format!("{path} labels a finding as fact without a cited evidence URL"),
                ));
            }
            for (key, child) in object {
                let child_path = format!("{path}.{key}");
                if key == "source_urls" {
                    let urls = child.as_array().ok_or_else(|| {
                        ToolExecutionError::new(
                            "capability_schema_mismatch",
                            format!("{child_path} must be an array"),
                        )
                    })?;
                    for url in urls.iter().filter_map(Value::as_str) {
                        if !allowed_urls.contains(url) {
                            return Err(ToolExecutionError::new(
                                "capability_schema_mismatch",
                                format!("{child_path} contains an unavailable evidence URL"),
                            ));
                        }
                    }
                } else {
                    validate_evidence_at(child, allowed_urls, &child_path)?;
                }
            }
        }
        Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                validate_evidence_at(child, allowed_urls, &format!("{path}[{index}]"))?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn has_evidence_source(evidence: Option<&Value>) -> bool {
    evidence
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("source_urls").and_then(Value::as_array))
        .flatten()
        .any(|url| url.as_str().is_some_and(|value| !value.is_empty()))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn evidence_urls_must_exist_in_the_frozen_research_context() {
        let context = json!({
            "research_sources": [{ "url": "https://example.com/filing" }]
        });
        let allowed = context_source_urls(&context);
        validate_evidence_urls(
            &json!({ "evidence": [{ "source_urls": ["https://example.com/filing"] }] }),
            &allowed,
        )
        .expect("known evidence URL");

        let error = validate_evidence_urls(
            &json!({ "evidence": [{ "source_urls": ["https://invented.example/"] }] }),
            &allowed,
        )
        .expect_err("unknown evidence URL is rejected");
        assert_eq!(error.code(), "capability_schema_mismatch");
    }

    #[test]
    fn fact_findings_require_at_least_one_evidence_url() {
        let error = validate_evidence_urls(
            &json!({
                "claim_type": "fact",
                "evidence": [{ "claim": "unsupported", "source_urls": [] }]
            }),
            &HashSet::new(),
        )
        .expect_err("an uncited fact is rejected");
        assert_eq!(error.code(), "capability_schema_mismatch");

        validate_evidence_urls(
            &json!({
                "claim_type": "inference",
                "evidence": [{ "claim": "explicit inference", "source_urls": [] }]
            }),
            &HashSet::new(),
        )
        .expect("an explicitly labeled inference may be uncited");
    }
}
