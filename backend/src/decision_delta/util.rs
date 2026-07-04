fn positive(value: Option<f64>, field: &str) -> AppResult<f64> {
    match value {
        Some(value) if value.is_finite() && value > 0.0 => Ok(value),
        _ => Err(AppError::bad_request(format!(
            "{field} must be greater than 0"
        ))),
    }
}

fn quantity_from_input(quantity: Option<f64>, notional: Option<f64>, price: f64) -> AppResult<f64> {
    match (quantity, notional) {
        (Some(quantity), _) if quantity.is_finite() && quantity > 0.0 => Ok(quantity),
        (None, Some(notional)) if notional.is_finite() && notional > 0.0 => Ok(notional / price),
        _ => Err(AppError::bad_request("quantity or notional is required")),
    }
}

fn clean_option(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn clean_list(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

fn matching_candidates(selected: Vec<String>, candidates: &[String]) -> Vec<String> {
    selected
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| candidates.iter().any(|candidate| candidate == value))
        .collect()
}

fn dedupe(values: Vec<String>) -> Vec<String> {
    let mut deduped = Vec::new();
    for value in values {
        if !deduped.contains(&value) {
            deduped.push(value);
        }
    }
    deduped
}

fn parse_bool(value: Option<&str>) -> Option<bool> {
    match value?.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" => Some(true),
        "false" | "0" | "no" => Some(false),
        _ => None,
    }
}

fn snapshot_limit(value: Option<usize>) -> usize {
    value
        .filter(|limit| *limit > 0)
        .unwrap_or(DEFAULT_SNAPSHOT_LIMIT)
        .min(MAX_SNAPSHOT_LIMIT)
}

fn round_money(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}
