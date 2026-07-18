use serde_json::Value;

const SUPPORTED_SCHEMA_KEYS: &[&str] = &[
    "type",
    "enum",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "maxItems",
    "maxLength",
    "description",
];

pub(crate) fn validate_schema_contract(schema: &Value, label: &str) -> Result<(), String> {
    validate_schema_at(schema, label)
}

pub(crate) fn validate_json_schema(
    value: &Value,
    schema: &Value,
    label: &str,
) -> Result<(), String> {
    validate_at(value, schema, label)
}

fn validate_at(value: &Value, schema: &Value, path: &str) -> Result<(), String> {
    let object = schema
        .as_object()
        .ok_or_else(|| format!("{path} schema must be an object"))?;
    if let Some(variants) = object.get("anyOf") {
        let variants = variants
            .as_array()
            .ok_or_else(|| format!("{path}.anyOf must be an array"))?;
        if variants.is_empty() {
            return Err(format!("{path}.anyOf cannot be empty"));
        }
        if !variants
            .iter()
            .any(|variant| validate_at(value, variant, path).is_ok())
        {
            return Err(format!("{path} does not match any allowed schema"));
        }
    }
    if let Some(allowed) = object.get("enum").and_then(Value::as_array) {
        if !allowed.iter().any(|candidate| candidate == value) {
            return Err(format!("{path} is not one of the allowed values"));
        }
    }
    if let Some(expected) = object.get("type").and_then(Value::as_str) {
        let matches = match expected {
            "object" => value.is_object(),
            "array" => value.is_array(),
            "string" => value.is_string(),
            "number" => value.is_number(),
            "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
            "boolean" => value.is_boolean(),
            "null" => value.is_null(),
            other => return Err(format!("{path} uses unsupported schema type '{other}'")),
        };
        if !matches {
            return Err(format!("{path} must be {expected}"));
        }
    }
    if let Some(value_object) = value.as_object() {
        let properties = object
            .get("properties")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        if let Some(required) = object.get("required").and_then(Value::as_array) {
            for field in required.iter().filter_map(Value::as_str) {
                if !value_object.contains_key(field) {
                    return Err(format!("{path}.{field} is required"));
                }
            }
        }
        for (field, field_value) in value_object {
            if let Some(field_schema) = properties.get(field) {
                validate_at(field_value, field_schema, &format!("{path}.{field}"))?;
            } else if object.get("additionalProperties") == Some(&Value::Bool(false)) {
                return Err(format!("{path}.{field} is not allowed"));
            }
        }
    }
    if let Some(items) = value.as_array() {
        if let Some(item_schema) = object.get("items") {
            for (index, item) in items.iter().enumerate() {
                validate_at(item, item_schema, &format!("{path}[{index}]"))?;
            }
        }
        if let Some(limit) = object.get("maxItems").and_then(Value::as_u64) {
            if items.len() > limit as usize {
                return Err(format!("{path} exceeds maxItems {limit}"));
            }
        }
    }
    if let Some(text) = value.as_str() {
        if let Some(limit) = object.get("maxLength").and_then(Value::as_u64) {
            if text.chars().count() > limit as usize {
                return Err(format!("{path} exceeds maxLength {limit}"));
            }
        }
    }
    Ok(())
}

fn validate_schema_at(schema: &Value, path: &str) -> Result<(), String> {
    let object = schema
        .as_object()
        .ok_or_else(|| format!("{path} must be a JSON object"))?;
    for key in object.keys() {
        if !SUPPORTED_SCHEMA_KEYS.contains(&key.as_str()) {
            return Err(format!("{path} uses unsupported schema keyword '{key}'"));
        }
    }
    if let Some(schema_type) = object.get("type") {
        let schema_type = schema_type
            .as_str()
            .ok_or_else(|| format!("{path}.type must be a string"))?;
        if ![
            "object", "array", "string", "number", "integer", "boolean", "null",
        ]
        .contains(&schema_type)
        {
            return Err(format!(
                "{path} uses unsupported schema type '{schema_type}'"
            ));
        }
    }
    if let Some(properties) = object.get("properties") {
        let properties = properties
            .as_object()
            .ok_or_else(|| format!("{path}.properties must be an object"))?;
        for (key, property) in properties {
            validate_schema_at(property, &format!("{path}.properties.{key}"))?;
        }
    }
    if let Some(required) = object.get("required") {
        let required = required
            .as_array()
            .ok_or_else(|| format!("{path}.required must be an array"))?;
        let properties = object.get("properties").and_then(Value::as_object);
        let mut seen = std::collections::HashSet::new();
        for field in required {
            let field = field
                .as_str()
                .ok_or_else(|| format!("{path}.required values must be strings"))?;
            if !seen.insert(field) {
                return Err(format!(
                    "{path}.required contains duplicate field '{field}'"
                ));
            }
            if properties.is_some_and(|properties| !properties.contains_key(field)) {
                return Err(format!(
                    "{path}.required references unknown field '{field}'"
                ));
            }
        }
    }
    if let Some(additional) = object.get("additionalProperties") {
        if !additional.is_boolean() {
            return Err(format!("{path}.additionalProperties must be boolean"));
        }
    }
    if let Some(items) = object.get("items") {
        validate_schema_at(items, &format!("{path}.items"))?;
    }
    for key in ["maxItems", "maxLength"] {
        if let Some(limit) = object.get(key) {
            if limit.as_u64().is_none() {
                return Err(format!("{path}.{key} must be a non-negative integer"));
            }
        }
    }
    if let Some(values) = object.get("enum") {
        let values = values
            .as_array()
            .ok_or_else(|| format!("{path}.enum must be an array"))?;
        if values.is_empty() {
            return Err(format!("{path}.enum cannot be empty"));
        }
    }
    Ok(())
}
