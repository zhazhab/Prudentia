fn supported_image_extension(file_name: &str, mime_type: Option<&str>) -> Option<&'static str> {
    match mime_type.map(|value| value.trim().to_ascii_lowercase()) {
        Some(value) if value == "image/png" => return Some("png"),
        Some(value) if value == "image/jpeg" || value == "image/jpg" => return Some("jpg"),
        Some(value) if value == "image/webp" => return Some("webp"),
        Some(value) if !value.is_empty() => return None,
        _ => {}
    }

    let lower_name = file_name.trim().to_ascii_lowercase();
    if lower_name.ends_with(".png") {
        Some("png")
    } else if lower_name.ends_with(".jpg") || lower_name.ends_with(".jpeg") {
        Some("jpg")
    } else if lower_name.ends_with(".webp") {
        Some("webp")
    } else {
        None
    }
}

fn clean_image_draft_row(mut row: PortfolioImageDraftRow) -> PortfolioImageDraftRow {
    row.symbol = row.symbol.trim().to_ascii_uppercase();
    row.name = row.name.trim().to_string();
    row.quantity = clean_numeric_text(&row.quantity);
    row.average_cost = clean_numeric_text(&row.average_cost);
    row.currency = normalize_currency_code(&row.currency);
    row.account = clean_optional_string(row.account);
    row.market = clean_optional_string(row.market).map(|value| normalize_market(&value));
    row.sector = clean_optional_string(row.sector);
    row.imported_market_value =
        clean_optional_string(row.imported_market_value).map(|value| clean_numeric_text(&value));
    row.last_price = clean_optional_string(row.last_price)
        .map(|value| clean_numeric_text(&value))
        .or_else(|| extract_visible_last_price(row.notes.as_deref()));
    row.notes = clean_optional_string(row.notes);
    row.confidence = match row.confidence.trim().to_ascii_lowercase().as_str() {
        "high" | "medium" | "low" | "unknown" => row.confidence.trim().to_ascii_lowercase(),
        _ => "unknown".to_string(),
    };
    row.warnings = row
        .warnings
        .into_iter()
        .map(|warning| warning.trim().to_string())
        .filter(|warning| !warning.is_empty())
        .collect();
    row
}

fn clean_numeric_text(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let first_value = trimmed
        .split_once('/')
        .map(|(first, _)| first)
        .unwrap_or(trimmed);
    let normalized = first_value
        .replace(',', "")
        .replace("HK$", "")
        .replace("US$", "")
        .replace("CN¥", "")
        .replace("HKD", "")
        .replace("USD", "")
        .replace("CNY", "")
        .replace("RMB", "")
        .replace(['$', '¥', '￥', '港', '元', '股', ' '], "");
    let numeric = normalized
        .chars()
        .filter(|value| value.is_ascii_digit() || matches!(value, '.' | '-' | '+'))
        .collect::<String>();

    if numeric.is_empty() {
        trimmed.to_string()
    } else {
        numeric.trim_start_matches('+').to_string()
    }
}

fn extract_visible_last_price(text: Option<&str>) -> Option<String> {
    let text = text?;
    let lower = text.to_ascii_lowercase();
    ["current_price", "current price", "last_price", "last price", "现价"]
        .iter()
        .filter_map(|marker| lower.find(marker).map(|index| index + marker.len()))
        .find_map(|start| first_number_after(&text[start..]))
}

fn first_number_after(text: &str) -> Option<String> {
    let mut number = String::new();
    let mut started = false;
    for value in text.chars() {
        if value.is_ascii_digit() || matches!(value, '.' | '-' | '+' | ',') {
            number.push(value);
            started = true;
            continue;
        }
        if started {
            break;
        }
    }

    if number.is_empty() {
        None
    } else {
        let cleaned = clean_numeric_text(&number);
        (!cleaned.is_empty()).then_some(cleaned)
    }
}

fn normalize_currency_code(value: &str) -> String {
    let normalized = value.trim().to_ascii_uppercase().replace('＄', "$");
    match normalized.as_str() {
        "" => String::new(),
        "HK$" | "HKD" | "港币" | "港元" => "HKD".to_string(),
        "CNY" | "CN¥" | "RMB" | "人民币" | "¥" | "￥" => "CNY".to_string(),
        "USD" | "US$" | "$" => "USD".to_string(),
        other => other.to_string(),
    }
}

fn infer_image_row_market(row: &PortfolioImageDraftRow, currency: &str) -> Option<String> {
    match currency {
        "HKD" => return Some("HK".to_string()),
        "USD" => return Some("US".to_string()),
        "CNY" => return Some("CN".to_string()),
        _ => {}
    }

    let context = image_row_context(row);
    if context.contains("HK$")
        || context.contains("港股")
        || context.contains("沪港")
        || context.contains("深港")
    {
        return Some("HK".to_string());
    }
    if context.contains("A股") || context.contains("人民币") || context.contains("CNY") {
        return Some("CN".to_string());
    }
    if context.contains("US$") || context.contains("USD") || context.contains("美股") {
        return Some("US".to_string());
    }
    if row
        .name
        .chars()
        .any(|value| matches!(value, '\u{4e00}'..='\u{9fff}'))
    {
        return Some("CN".to_string());
    }
    None
}

fn infer_image_row_currency(row: &PortfolioImageDraftRow, market: &str) -> Option<String> {
    inferred_currency(&row.symbol, Some(market)).or_else(|| {
        let context = image_row_context(row);
        if context.contains("HK$") || context.contains("港币") || context.contains("港元") {
            Some("HKD".to_string())
        } else if context.contains("US$") || context.contains("USD") {
            Some("USD".to_string())
        } else if context.contains("人民币")
            || context.contains("CNY")
            || row
                .name
                .chars()
                .any(|value| matches!(value, '\u{4e00}'..='\u{9fff}'))
        {
            Some("CNY".to_string())
        } else {
            None
        }
    })
}

fn image_row_context(row: &PortfolioImageDraftRow) -> String {
    [
        row.name.as_str(),
        row.symbol.as_str(),
        row.currency.as_str(),
        row.account.as_deref().unwrap_or_default(),
        row.market.as_deref().unwrap_or_default(),
        row.notes.as_deref().unwrap_or_default(),
        row.average_cost.as_str(),
        row.imported_market_value.as_deref().unwrap_or_default(),
        row.last_price.as_deref().unwrap_or_default(),
    ]
    .join(" ")
    .to_ascii_uppercase()
}

fn is_non_security_image_row(row: &PortfolioImageDraftRow) -> bool {
    let normalized_name = row.name.trim().to_ascii_lowercase();
    if normalized_name.is_empty() {
        return true;
    }

    let has_position_metrics = !row.quantity.trim().is_empty()
        && !row.average_cost.trim().is_empty()
        && row
            .imported_market_value
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty());
    if has_position_metrics {
        return false;
    }

    ["现金", "余额", "可用资金", "可取资金", "cash", "balance"]
        .iter()
        .any(|marker| normalized_name.contains(marker))
}
