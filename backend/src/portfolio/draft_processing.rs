fn read_tabular_content(
    file_name: &str,
    content: &str,
    content_encoding: Option<String>,
) -> AppResult<TabularContent> {
    if file_name.ends_with(".xlsx") {
        return read_xlsx(content, content_encoding);
    }

    let bytes = if matches!(content_encoding.as_deref(), Some("base64")) {
        general_purpose::STANDARD.decode(content)?
    } else {
        content.as_bytes().to_vec()
    };
    let plain = String::from_utf8(bytes)
        .map_err(|_| AppError::bad_request("import content must be valid UTF-8"))?;
    read_delimited(file_name, &plain)
}

fn read_delimited(file_name: &str, content: &str) -> AppResult<TabularContent> {
    let delimiter = if file_name.ends_with(".tsv") {
        b'\t'
    } else {
        b','
    };
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .delimiter(delimiter)
        .from_reader(content.as_bytes());
    let headers = reader
        .headers()?
        .iter()
        .map(|value| value.trim().to_string())
        .collect::<Vec<_>>();
    let mut rows = Vec::new();

    for record in reader.records() {
        rows.push(
            record?
                .iter()
                .map(|value| value.trim().to_string())
                .collect::<Vec<_>>(),
        );
    }

    Ok(TabularContent { headers, rows })
}

fn read_xlsx(content: &str, content_encoding: Option<String>) -> AppResult<TabularContent> {
    if !matches!(content_encoding.as_deref(), Some("base64")) {
        return Err(AppError::bad_request(
            "xlsx imports must send content_encoding=base64",
        ));
    }

    let bytes = general_purpose::STANDARD.decode(content)?;
    let cursor = Cursor::new(bytes);
    let mut workbook: Xlsx<_> =
        Xlsx::new(cursor).map_err(|err| AppError::bad_request(err.to_string()))?;
    let range = workbook
        .worksheet_range_at(0)
        .ok_or_else(|| AppError::bad_request("xlsx workbook has no worksheets"))?
        .map_err(|err| AppError::bad_request(err.to_string()))?;
    let mut rows = range.rows().map(cells_to_strings).collect::<Vec<_>>();

    if rows.is_empty() {
        return Ok(TabularContent {
            headers: Vec::new(),
            rows: Vec::new(),
        });
    }

    let headers = rows.remove(0);
    Ok(TabularContent { headers, rows })
}

fn cells_to_strings(row: &[calamine::Data]) -> Vec<String> {
    row.iter().map(|cell| cell.to_string()).collect()
}

fn draft_rows_from_table(
    headers: &[String],
    rows: &[Vec<String>],
    mapping: &PortfolioImportMapping,
) -> Vec<PortfolioDraftRow> {
    rows.iter()
        .map(|row| draft_row_from_table_row(headers, row, mapping))
        .collect()
}

fn draft_row_from_table_row(
    headers: &[String],
    row: &[String],
    mapping: &PortfolioImportMapping,
) -> PortfolioDraftRow {
    let symbol = required_or_empty(headers, row, &mapping.symbol).to_ascii_uppercase();
    let mapped_market = optional_cell(headers, row, mapping.market.as_deref())
        .map(|value| normalize_market(&value));
    let inferred_market = mapped_market.or_else(|| infer_market(&symbol));
    let mapped_currency = required_or_empty(headers, row, &mapping.currency).to_ascii_uppercase();
    let currency = if mapped_currency.is_empty() {
        inferred_currency(&symbol, inferred_market.as_deref()).unwrap_or_default()
    } else {
        mapped_currency
    };
    let market = inferred_market.unwrap_or_else(|| "Other".to_string());

    normalize_and_validate_draft_row(PortfolioDraftRow {
        symbol,
        name: required_or_empty(headers, row, &mapping.name),
        quantity: required_or_empty(headers, row, &mapping.quantity),
        average_cost: required_or_empty(headers, row, &mapping.average_cost),
        currency,
        account: optional_cell(headers, row, mapping.account.as_deref()),
        market,
        sector: optional_cell(headers, row, mapping.sector.as_deref()),
        imported_market_value: optional_cell(
            headers,
            row,
            mapping.imported_market_value.as_deref(),
        ),
        last_price: None,
        notes: optional_cell(headers, row, mapping.notes.as_deref()),
        confidence: "high".to_string(),
        warnings: Vec::new(),
        errors: Vec::new(),
    })
}

fn draft_row_from_image_row(row: PortfolioImageDraftRow) -> PortfolioDraftRow {
    let symbol = row.symbol.to_ascii_uppercase();
    let row_currency = normalize_currency_code(&row.currency);
    let market = row
        .market
        .as_deref()
        .map(normalize_market)
        .filter(|value| !value.is_empty() && value != "Other")
        .or_else(|| infer_market(&symbol))
        .or_else(|| infer_image_row_market(&row, &row_currency))
        .unwrap_or_else(|| "Other".to_string());
    let currency = if row_currency.is_empty() {
        inferred_currency(&symbol, Some(&market))
            .unwrap_or_else(|| infer_image_row_currency(&row, &market).unwrap_or_default())
    } else {
        row_currency
    };

    normalize_and_validate_draft_row(PortfolioDraftRow {
        symbol,
        name: row.name,
        quantity: row.quantity,
        average_cost: row.average_cost,
        currency,
        account: row.account,
        market,
        sector: row.sector,
        imported_market_value: row.imported_market_value,
        last_price: row.last_price,
        notes: row.notes,
        confidence: row.confidence,
        warnings: row.warnings,
        errors: Vec::new(),
    })
}

fn clean_image_recognition_rows(rows: Vec<PortfolioImageDraftRow>) -> Vec<PortfolioImageDraftRow> {
    rows.into_iter()
        .map(clean_image_draft_row)
        .filter(|row| !is_non_security_image_row(row))
        .collect()
}

fn draft_rows_from_image_recognition_rows(
    rows: Vec<PortfolioImageDraftRow>,
) -> Vec<PortfolioDraftRow> {
    clean_image_recognition_rows(rows)
        .into_iter()
        .map(draft_row_from_image_row)
        .collect()
}

fn normalize_and_validate_draft_row(row: PortfolioDraftRow) -> PortfolioDraftRow {
    let mut row = normalize_draft_row(row);
    row.errors = validate_draft_row(&row);
    row
}

fn normalize_draft_row(mut row: PortfolioDraftRow) -> PortfolioDraftRow {
    row.name = row.name.trim().to_string();
    row.quantity = row.quantity.trim().to_string();
    row.average_cost = row.average_cost.trim().to_string();
    row.currency = row.currency.trim().to_ascii_uppercase();
    row.account = clean_optional_string(row.account);
    row.market = normalize_market(&row.market);
    row.symbol = normalize_security_symbol_input(&row.symbol, &row.market, &row.currency);
    row.sector = clean_optional_string(row.sector);
    row.imported_market_value = clean_optional_string(row.imported_market_value);
    row.last_price = clean_optional_string(row.last_price);
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
    row.errors = row
        .errors
        .into_iter()
        .map(|error| error.trim().to_string())
        .filter(|error| !error.is_empty())
        .collect();
    row
}

fn validate_draft_row(row: &PortfolioDraftRow) -> Vec<String> {
    let mut errors = Vec::new();
    if row.name.trim().is_empty() {
        errors.push("name is required".to_string());
    }
    match parse_positive_f64(&row.quantity, "quantity") {
        Ok(_) => {}
        Err(error) => errors.push(error.to_string()),
    }
    match parse_non_negative_f64(&row.average_cost, "average_cost") {
        Ok(_) => {}
        Err(error) => errors.push(error.to_string()),
    }
    if row.currency.trim().is_empty() {
        errors.push("currency is required".to_string());
    }
    if row.market.trim().is_empty() {
        errors.push("market is required".to_string());
    }
    if let Some(imported_market_value) = &row.imported_market_value {
        if let Err(error) = parse_non_negative_f64(imported_market_value, "imported_market_value") {
            errors.push(error.to_string());
        }
    }
    if let Some(last_price) = &row.last_price {
        if let Err(error) = parse_non_negative_f64(last_price, "last_price") {
            errors.push(error.to_string());
        }
    }
    errors
}

fn position_from_draft_row(row: &PortfolioDraftRow) -> AppResult<PortfolioPosition> {
    if row.symbol.trim().is_empty() {
        return Err(AppError::bad_request(format!(
            "symbol could not be resolved for company name {}",
            row.name
        )));
    }

    let quantity = parse_positive_f64(&row.quantity, "quantity")?;
    let average_cost = parse_non_negative_f64(&row.average_cost, "average_cost")?;
    let imported_market_value = row
        .imported_market_value
        .as_deref()
        .and_then(|value| parse_non_negative_f64(value, "imported_market_value").ok());
    let visible_last_price = explicit_last_price(row);
    let last_price = visible_last_price
        .or_else(|| imported_market_value.map(|value| ratio(value, quantity)))
        .or(Some(average_cost));
    let market_value = visible_last_price
        .map(|price| price * quantity)
        .or(imported_market_value)
        .unwrap_or(quantity * average_cost);
    let cost_basis = quantity * average_cost;

    Ok(PortfolioPosition {
        symbol: row.symbol.to_ascii_uppercase(),
        name: row.name.clone(),
        asset_type: "stock".to_string(),
        quantity,
        average_cost,
        currency: row.currency.to_ascii_uppercase(),
        account: row.account.clone(),
        market: Some(normalize_market(&row.market)),
        sector: row.sector.clone(),
        notes: row.notes.clone(),
        last_price,
        market_value,
        unrealized_pnl: market_value - cost_basis,
        weight: 0.0,
        price_updated_at: None,
        price_stale: true,
        updated_at: now_iso(),
    })
}

fn position_from_row(
    headers: &[String],
    row: &[String],
    mapping: &PortfolioImportMapping,
) -> AppResult<PortfolioPosition> {
    let symbol = required_cell(headers, row, &mapping.symbol)?.to_uppercase();
    let name = required_cell(headers, row, &mapping.name)?;
    let quantity =
        parse_positive_f64(&required_cell(headers, row, &mapping.quantity)?, "quantity")?;
    let average_cost = parse_non_negative_f64(
        &required_cell(headers, row, &mapping.average_cost)?,
        "average_cost",
    )?;
    let currency = required_cell(headers, row, &mapping.currency)?.to_uppercase();
    let imported_market_value =
        optional_cell(headers, row, mapping.imported_market_value.as_deref())
            .and_then(|value| parse_non_negative_f64(&value, "imported_market_value").ok());
    let last_price = imported_market_value
        .map(|value| ratio(value, quantity))
        .or(Some(average_cost));
    let market_value = imported_market_value.unwrap_or(quantity * average_cost);
    let cost_basis = quantity * average_cost;

    Ok(PortfolioPosition {
        symbol,
        name,
        asset_type: "stock".to_string(),
        quantity,
        average_cost,
        currency,
        account: optional_cell(headers, row, mapping.account.as_deref()),
        market: optional_cell(headers, row, mapping.market.as_deref()),
        sector: optional_cell(headers, row, mapping.sector.as_deref()),
        notes: optional_cell(headers, row, mapping.notes.as_deref()),
        last_price,
        market_value,
        unrealized_pnl: market_value - cost_basis,
        weight: 0.0,
        price_updated_at: None,
        price_stale: true,
        updated_at: now_iso(),
    })
}

fn validate_mapping(headers: &[String], mapping: &PortfolioImportMapping) -> Vec<String> {
    let mut errors = Vec::new();
    for (field, header) in [
        ("symbol", &mapping.symbol),
        ("name", &mapping.name),
        ("quantity", &mapping.quantity),
        ("average_cost", &mapping.average_cost),
        ("currency", &mapping.currency),
    ] {
        if header.trim().is_empty() {
            errors.push(format!("{field} mapping is required"));
        } else if column_index(headers, header).is_none() {
            errors.push(format!(
                "{field} mapping points to missing column '{header}'"
            ));
        }
    }
    errors
}

fn suggest_mapping(headers: &[String]) -> PortfolioImportMapping {
    PortfolioImportMapping {
        symbol: find_header(headers, &["symbol", "ticker", "代码", "证券代码"]).unwrap_or_default(),
        name: find_header(headers, &["name", "security", "证券名称", "名称"]).unwrap_or_default(),
        quantity: find_header(headers, &["quantity", "shares", "持仓", "数量"]).unwrap_or_default(),
        average_cost: find_header(
            headers,
            &["average cost", "avg cost", "cost", "成本", "成本价"],
        )
        .unwrap_or_default(),
        currency: find_header(headers, &["currency", "币种"]).unwrap_or_default(),
        account: find_header(headers, &["account", "账户"]),
        market: find_header(headers, &["market", "exchange", "市场"]),
        sector: find_header(headers, &["sector", "行业"]),
        imported_market_value: find_header(headers, &["market value", "市值"]),
        notes: find_header(headers, &["notes", "备注"]),
    }
}

fn find_header(headers: &[String], candidates: &[&str]) -> Option<String> {
    headers.iter().find_map(|header| {
        let normalized = normalize_header(header);
        candidates
            .iter()
            .any(|candidate| normalized == normalize_header(candidate))
            .then(|| header.clone())
    })
}

fn required_cell(headers: &[String], row: &[String], header: &str) -> AppResult<String> {
    optional_cell(headers, row, Some(header))
        .ok_or_else(|| AppError::bad_request(format!("missing required value for {header}")))
}

fn required_or_empty(headers: &[String], row: &[String], header: &str) -> String {
    optional_cell(headers, row, Some(header)).unwrap_or_default()
}

fn optional_cell(headers: &[String], row: &[String], header: Option<&str>) -> Option<String> {
    let header = header?;
    let index = column_index(headers, header)?;
    row.get(index)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn column_index(headers: &[String], header: &str) -> Option<usize> {
    let target = normalize_header(header);
    headers
        .iter()
        .position(|candidate| normalize_header(candidate) == target)
}

fn row_to_map(headers: &[String], row: &[String]) -> HashMap<String, String> {
    headers
        .iter()
        .enumerate()
        .map(|(index, header)| (header.clone(), row.get(index).cloned().unwrap_or_default()))
        .collect()
}

fn parse_positive_f64(value: &str, field: &str) -> AppResult<f64> {
    let normalized = value.replace(',', "");
    let parsed = normalized
        .parse::<f64>()
        .map_err(|_| AppError::bad_request(format!("{field} must be a number")))?;
    if parsed <= 0.0 {
        return Err(AppError::bad_request(format!(
            "{field} must be greater than 0"
        )));
    }
    Ok(parsed)
}

fn parse_non_negative_f64(value: &str, field: &str) -> AppResult<f64> {
    let normalized = value.replace(',', "");
    let parsed = normalized
        .parse::<f64>()
        .map_err(|_| AppError::bad_request(format!("{field} must be a number")))?;
    if parsed < 0.0 {
        return Err(AppError::bad_request(format!(
            "{field} must be non-negative"
        )));
    }
    Ok(parsed)
}

fn position_from_db_row(row: sqlx::sqlite::SqliteRow) -> AppResult<PortfolioPosition> {
    Ok(PortfolioPosition {
        symbol: row.try_get("symbol")?,
        name: row.try_get("name")?,
        asset_type: row.try_get("asset_type")?,
        quantity: row.try_get("quantity")?,
        average_cost: row.try_get("average_cost")?,
        currency: row.try_get("currency")?,
        account: row.try_get("account")?,
        market: row.try_get("market")?,
        sector: row.try_get("sector")?,
        notes: row.try_get("notes")?,
        last_price: row.try_get("last_price")?,
        market_value: row.try_get("market_value")?,
        unrealized_pnl: row.try_get("unrealized_pnl")?,
        weight: row.try_get("weight")?,
        price_updated_at: row.try_get("price_updated_at")?,
        price_stale: row.try_get::<i64, _>("price_stale")? != 0,
        updated_at: row.try_get("updated_at")?,
    })
}

fn ratio(value: f64, denominator: f64) -> f64 {
    if denominator.abs() < f64::EPSILON {
        0.0
    } else {
        value / denominator
    }
}

fn normalize_header(value: &str) -> String {
    value.trim().to_lowercase().replace([' ', '_', '-'], "")
}

fn normalize_market(value: &str) -> String {
    let normalized = value.trim();
    match normalized.to_ascii_uppercase().as_str() {
        "US" | "USA" | "NYSE" | "NASDAQ" => "US".to_string(),
        "HK" | "HKG" | "HKEX" | "香港" => "HK".to_string(),
        "CN" | "CHINA" | "SH" | "SHANGHAI" | "SZ" | "SHENZHEN" | "沪深" | "A股" => {
            "CN".to_string()
        }
        "OTHER" | "其他" => "Other".to_string(),
        "" => String::new(),
        _ if normalized.contains("港股")
            || normalized.contains("沪港")
            || normalized.contains("深港") =>
        {
            "HK".to_string()
        }
        _ if normalized.contains('港') => "HK".to_string(),
        _ if normalized.contains("A股")
            || normalized.contains('沪')
            || normalized.contains('深') =>
        {
            "CN".to_string()
        }
        other => other.to_string(),
    }
}

fn infer_market(symbol: &str) -> Option<String> {
    let symbol = symbol.trim().to_ascii_uppercase();
    if symbol.is_empty() {
        return None;
    }
    if symbol.ends_with(".HK")
        || (symbol.chars().all(|value| value.is_ascii_digit()) && symbol.len() <= 5)
    {
        return Some("HK".to_string());
    }
    if symbol.ends_with(".SS")
        || symbol.ends_with(".SH")
        || symbol.ends_with(".SHH")
        || symbol.ends_with(".SZ")
        || symbol.ends_with(".SHE")
        || symbol.ends_with(".SHZ")
        || (symbol.len() == 6
            && symbol.chars().all(|value| value.is_ascii_digit())
            && matches!(symbol.as_bytes()[0], b'0' | b'3' | b'6'))
    {
        return Some("CN".to_string());
    }
    if symbol
        .chars()
        .all(|value| value.is_ascii_alphabetic() || value == '.')
    {
        return Some("US".to_string());
    }
    None
}

fn inferred_currency(symbol: &str, market: Option<&str>) -> Option<String> {
    match market.map(normalize_market).as_deref() {
        Some("US") => Some("USD".to_string()),
        Some("HK") => Some("HKD".to_string()),
        Some("CN") => Some("CNY".to_string()),
        _ => infer_market(symbol).and_then(|market| inferred_currency("", Some(&market))),
    }
}

fn explicit_last_price(row: &PortfolioDraftRow) -> Option<f64> {
    row.last_price
        .as_deref()
        .and_then(|value| parse_non_negative_f64(value, "last_price").ok())
        .or_else(|| {
            extract_visible_last_price(row.notes.as_deref())
                .as_deref()
                .and_then(|value| parse_non_negative_f64(value, "last_price").ok())
        })
}

fn clean_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(clean_string)
}

fn clean_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

struct TemporaryImportFile {
    path: PathBuf,
}

impl TemporaryImportFile {
    fn write(prefix: &str, extension: &str, bytes: &[u8]) -> AppResult<Self> {
        let file_name = format!("{prefix}-{}.{}", Uuid::new_v4(), extension);
        let path = std::env::temp_dir().join(file_name);
        fs::write(&path, bytes)
            .map_err(|err| AppError::internal(format!("failed to write temporary image: {err}")))?;
        Ok(Self { path })
    }
}

impl Drop for TemporaryImportFile {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_file(&self.path) {
            tracing::debug!(path = %self.path.display(), error = %error, "temporary image cleanup failed");
        }
    }
}

struct TabularContent {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}
