#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecuritySymbol {
    pub symbol: String,
    pub name: String,
    pub market: String,
    pub currency: String,
    #[serde(skip_serializing)]
    pub asset_type: String,
    #[serde(skip_serializing)]
    pub exchange: Option<String>,
    #[serde(skip_serializing)]
    pub provider: String,
    #[serde(skip_serializing)]
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SecuritySymbolSearchQuery {
    pub q: Option<String>,
    pub market: Option<String>,
    pub currency: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SecuritySymbolRefreshResult {
    pub provider: String,
    pub upserted_count: usize,
    pub skipped_count: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PortfolioDraftSymbolResolveRequest {
    pub rows: Vec<PortfolioDraftRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PortfolioDraftSymbolResolveResult {
    pub draft_rows: Vec<PortfolioDraftRow>,
    pub resolved_count: usize,
}

#[derive(Debug, Clone)]
struct RankedSecuritySymbol {
    symbol: SecuritySymbol,
    score: i32,
}

#[derive(Debug, Deserialize)]
struct TushareResponse {
    code: i64,
    msg: Option<String>,
    data: Option<TushareData>,
}

#[derive(Debug, Deserialize)]
struct TushareData {
    fields: Vec<String>,
    items: Vec<Vec<serde_json::Value>>,
}

#[derive(Debug, Serialize)]
struct TushareRequest<'a> {
    api_name: &'a str,
    token: &'a str,
    params: serde_json::Value,
    fields: &'a str,
}

async fn search_security_symbols(
    pool: &SqlitePool,
    query: &SecuritySymbolSearchQuery,
) -> AppResult<Vec<SecuritySymbol>> {
    let q = query.q.as_deref().unwrap_or_default().trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }

    let limit = query.limit.unwrap_or(10).clamp(1, 50);
    Ok(ranked_security_symbols(
        pool,
        q,
        query.market.as_deref().unwrap_or_default(),
        query.currency.as_deref().unwrap_or_default(),
        limit,
    )
    .await?
    .into_iter()
    .map(|candidate| candidate.symbol)
    .collect())
}

async fn resolve_draft_symbols_from_directory(
    pool: &SqlitePool,
    request: PortfolioDraftSymbolResolveRequest,
) -> AppResult<PortfolioDraftSymbolResolveResult> {
    let mut draft_rows = request.rows;
    let resolved_count = resolve_missing_draft_symbols(pool, &mut draft_rows).await?;
    Ok(PortfolioDraftSymbolResolveResult {
        draft_rows,
        resolved_count,
    })
}

async fn refresh_security_symbols_from_config(
    pool: &SqlitePool,
) -> AppResult<SecuritySymbolRefreshResult> {
    let provider = std::env::var("SYMBOL_DIRECTORY_PROVIDER")
        .unwrap_or_else(|_| "public".to_string())
        .trim()
        .to_ascii_lowercase();

    refresh_security_symbols(pool, &provider).await
}

async fn refresh_security_symbols(
    pool: &SqlitePool,
    provider: &str,
) -> AppResult<SecuritySymbolRefreshResult> {
    match provider.trim().to_ascii_lowercase().as_str() {
        "public" | "exchange_public" => PublicSymbolDirectoryProvider::new()
            .refresh(pool)
            .await,
        "tushare" => refresh_tushare_symbols(pool).await,
        "local" | "" => Err(AppError::bad_request(
            "symbol directory provider is local; automatic refresh is disabled",
        )),
        other => Err(AppError::bad_request(format!(
            "unsupported symbol directory provider '{other}'"
        ))),
    }
}

async fn refresh_tushare_symbols(pool: &SqlitePool) -> AppResult<SecuritySymbolRefreshResult> {
    let token = std::env::var("TUSHARE_TOKEN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::bad_request("TUSHARE_TOKEN is required"))?;

    let provider = TushareSymbolDirectoryProvider::new(token);
    let mut result = provider.refresh(pool).await?;
    if result.upserted_count == 0 && !result.errors.is_empty() {
        return Err(AppError::bad_request(result.errors.join("; ")));
    }
    result.provider = "tushare".to_string();
    Ok(result)
}

pub fn start_symbol_directory_refresh_job(pool: SqlitePool, provider: String, interval: Duration) {
    tokio::spawn(async move {
        loop {
            match refresh_security_symbols(&pool, &provider).await {
                Ok(result) => tracing::info!(
                    provider = %result.provider,
                    upserted_count = result.upserted_count,
                    skipped_count = result.skipped_count,
                    error_count = result.errors.len(),
                    "security symbol directory refresh finished"
                ),
                Err(error) => tracing::warn!(
                    error = %error,
                    "security symbol directory refresh failed"
                ),
            }
            tokio::time::sleep(interval).await;
        }
    });
}

async fn resolve_missing_draft_symbols(
    pool: &SqlitePool,
    rows: &mut [PortfolioDraftRow],
) -> AppResult<usize> {
    let mut resolved_count = 0;
    let existing_positions = list_positions(pool).await?;
    for row in rows.iter_mut() {
        let original_symbol = row.symbol.clone();
        *row = normalize_and_validate_draft_row(row.clone());
        if !original_symbol.trim().is_empty() && row.symbol != original_symbol.trim() {
            resolved_count += 1;
        }
        if !row.symbol.trim().is_empty() || row.name.trim().is_empty() {
            continue;
        }

        if let Some(matched) = resolve_symbol_from_existing_positions(
            &existing_positions,
            &row.name,
            &row.market,
            &row.currency,
        ) {
            apply_resolved_symbol_to_draft_row(row, &matched.symbol);
            *row = normalize_and_validate_draft_row(row.clone());
            resolved_count += 1;
            continue;
        }

        if let Some(symbol) =
            resolve_security_symbol(pool, &row.name, &row.market, &row.currency).await?
        {
            apply_resolved_symbol_to_draft_row(row, &symbol.symbol);
            if row.market.trim().is_empty() {
                row.market = symbol.market;
            }
            if row.currency.trim().is_empty() {
                row.currency = symbol.currency;
            }
            *row = normalize_and_validate_draft_row(row.clone());
            resolved_count += 1;
        }
    }
    Ok(resolved_count)
}

async fn resolve_security_symbol(
    pool: &SqlitePool,
    company_name: &str,
    market: &str,
    currency: &str,
) -> AppResult<Option<SecuritySymbol>> {
    let candidates = ranked_security_symbols(pool, company_name, market, currency, 5).await?;
    let Some(best) = candidates.first() else {
        return Ok(None);
    };

    let normalized_query = normalize_symbol_lookup_text(company_name);
    let exact_name = normalize_symbol_lookup_text(&best.symbol.name) == normalized_query;
    let exact_symbol = symbol_lookup_query_variants(company_name, market, currency)
        .iter()
        .any(|query_symbol| best.symbol.symbol.eq_ignore_ascii_case(query_symbol));
    let uniquely_best = candidates
        .get(1)
        .map(|second| best.score > second.score)
        .unwrap_or(true);

    if (exact_name || exact_symbol) && uniquely_best {
        Ok(Some(best.symbol.clone()))
    } else {
        Ok(None)
    }
}

async fn ranked_security_symbols(
    pool: &SqlitePool,
    query: &str,
    market: &str,
    currency: &str,
    limit: usize,
) -> AppResult<Vec<RankedSecuritySymbol>> {
    let trimmed = query.trim();
    let mut rows = if contains_cjk(trimmed) {
        let mut rows = Vec::new();
        let mut seen_symbols = HashSet::new();
        for variant in chinese_lookup_variants(trimmed) {
            for row in fetch_security_symbol_like_rows(pool, &variant, market, currency, 200).await? {
                let symbol: String = row.try_get("symbol")?;
                if seen_symbols.insert(symbol) {
                    rows.push(row);
                }
            }
        }
        rows
    } else {
        let mut rows = Vec::new();
        let mut seen_symbols = HashSet::new();
        for variant in symbol_lookup_query_variants(trimmed, market, currency) {
            for row in fetch_security_symbol_like_rows(pool, &variant, market, currency, 200).await? {
                let symbol: String = row.try_get("symbol")?;
                if seen_symbols.insert(symbol) {
                    rows.push(row);
                }
            }
        }
        rows
    };
    if rows.is_empty() && contains_cjk(trimmed) {
        rows = fetch_security_symbol_hint_rows(pool, market, currency).await?;
    }

    let mut ranked = rows
        .into_iter()
        .map(security_symbol_from_row)
        .collect::<AppResult<Vec<_>>>()?
        .into_iter()
        .filter_map(|symbol| {
            let score = score_security_symbol(&symbol, trimmed, market, currency);
            (score > 0).then_some(RankedSecuritySymbol { symbol, score })
        })
        .collect::<Vec<_>>();

    ranked.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.symbol.symbol.cmp(&right.symbol.symbol))
    });
    ranked.truncate(limit);
    Ok(ranked)
}

async fn fetch_security_symbol_like_rows(
    pool: &SqlitePool,
    query: &str,
    market: &str,
    currency: &str,
    limit: i64,
) -> AppResult<Vec<sqlx::sqlite::SqliteRow>> {
    let like = format!("%{}%", query.trim());
    let symbol_like = format!("%{}%", query.trim().to_ascii_uppercase());
    let canonical_market = canonical_market_hint(market);
    let canonical_currency = currency.trim().to_ascii_uppercase();
    Ok(sqlx::query(
        r#"
        SELECT symbol, name, market, currency, updated_at
        FROM security_symbols
        WHERE (name LIKE ? OR symbol LIKE ?)
          AND (? = '' OR market = ?)
          AND (? = '' OR currency = ?)
        LIMIT ?
        "#,
    )
    .bind(&like)
    .bind(&symbol_like)
    .bind(&canonical_market)
    .bind(&canonical_market)
    .bind(&canonical_currency)
    .bind(&canonical_currency)
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

async fn fetch_security_symbol_hint_rows(
    pool: &SqlitePool,
    market: &str,
    currency: &str,
) -> AppResult<Vec<sqlx::sqlite::SqliteRow>> {
    let canonical_market = canonical_market_hint(market);
    let canonical_currency = currency.trim().to_ascii_uppercase();
    Ok(sqlx::query(
        r#"
        SELECT symbol, name, market, currency, updated_at
        FROM security_symbols
        WHERE (? = '' OR market = ?)
          AND (? = '' OR currency = ?)
        LIMIT 50000
        "#,
    )
    .bind(&canonical_market)
    .bind(&canonical_market)
    .bind(&canonical_currency)
    .bind(&canonical_currency)
    .fetch_all(pool)
    .await?)
}

async fn upsert_security_symbols(pool: &SqlitePool, symbols: &[SecuritySymbol]) -> AppResult<usize> {
    let mut upserted = 0;
    for symbol in symbols {
        sqlx::query(
            r#"
            INSERT INTO security_symbols (
                symbol, name, market, currency, updated_at
            )
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(symbol) DO UPDATE SET
                name = excluded.name,
                market = excluded.market,
                currency = excluded.currency,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&symbol.symbol)
        .bind(&symbol.name)
        .bind(&symbol.market)
        .bind(&symbol.currency)
        .bind(&symbol.updated_at)
        .execute(pool)
        .await?;
        upserted += 1;
    }
    Ok(upserted)
}

fn security_symbol_from_row(row: sqlx::sqlite::SqliteRow) -> AppResult<SecuritySymbol> {
    Ok(SecuritySymbol {
        symbol: row.try_get("symbol")?,
        name: row.try_get("name")?,
        market: row.try_get("market")?,
        currency: row.try_get("currency")?,
        asset_type: "security".to_string(),
        exchange: None,
        provider: "security_symbols".to_string(),
        updated_at: row.try_get("updated_at")?,
    })
}

fn score_security_symbol(
    symbol: &SecuritySymbol,
    query: &str,
    market: &str,
    currency: &str,
) -> i32 {
    let normalized_query = normalize_symbol_lookup_text(query);
    let normalized_name = normalize_symbol_lookup_text(&symbol.name);
    let normalized_symbol = symbol.symbol.to_ascii_uppercase();
    let normalized_query_symbols = symbol_lookup_query_variants(query, market, currency);
    let normalized_market = market.trim().to_ascii_uppercase();
    let normalized_currency = currency.trim().to_ascii_uppercase();
    let mut text_score = 0;

    if normalized_query_symbols.contains(&normalized_symbol) {
        text_score += 1000;
    }
    if normalized_name == normalized_query {
        text_score += 900;
    } else if !normalized_query.is_empty()
        && (normalized_name.contains(&normalized_query) || normalized_query.contains(&normalized_name))
    {
        text_score += 500;
    }

    if text_score == 0 {
        return 0;
    }

    let mut score = text_score;
    if market_matches(&symbol.market, &normalized_market) {
        score += 120;
    } else if !normalized_market.is_empty() {
        score -= 120;
    }

    if symbol.currency.eq_ignore_ascii_case(&normalized_currency) {
        score += 80;
    } else if !normalized_currency.is_empty() {
        score -= 80;
    }

    score
}

fn market_matches(symbol_market: &str, hint: &str) -> bool {
    match hint {
        "" => false,
        "CN" | "A股" => symbol_market == "CN",
        "HK" | "港股" => symbol_market == "HK",
        "US" | "美股" => symbol_market == "US",
        other => symbol_market.eq_ignore_ascii_case(other),
    }
}

fn canonical_market_hint(market: &str) -> String {
    match market.trim().to_ascii_uppercase().as_str() {
        "CN" | "A股" => "CN".to_string(),
        "HK" | "港股" => "HK".to_string(),
        "US" | "美股" => "US".to_string(),
        other => other.to_string(),
    }
}

fn normalize_symbol_lookup_text(value: &str) -> String {
    let value = simplified_chinese(value);
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(|character| character.to_lowercase())
        .collect()
}

fn symbol_lookup_query_variants(value: &str, market: &str, currency: &str) -> Vec<String> {
    let mut variants = Vec::new();
    push_unique_variant(&mut variants, value.trim().to_ascii_uppercase());
    push_unique_variant(
        &mut variants,
        normalize_security_symbol_input(value, market, currency),
    );
    variants
}

fn normalize_security_symbol_input(symbol: &str, market: &str, currency: &str) -> String {
    let upper = symbol.trim().to_ascii_uppercase();
    if upper.is_empty() {
        return upper;
    }

    if let Some(code) = upper.strip_suffix(".HK") {
        return normalize_hk_symbol_code(code).unwrap_or(upper);
    }
    if let Some(code) = upper.strip_suffix(".SH") {
        return format!("{code}.SS");
    }
    if upper.chars().all(|value| value.is_ascii_digit())
        && should_normalize_as_hk_code(&upper, market, currency)
    {
        return format!("{}.HK", normalize_hk_numeric_code(&upper));
    }
    upper
}

fn normalize_hk_symbol_code(code: &str) -> Option<String> {
    code.chars()
        .all(|value| value.is_ascii_digit())
        .then(|| format!("{}.HK", normalize_hk_numeric_code(code)))
}

fn should_normalize_as_hk_code(code: &str, market: &str, currency: &str) -> bool {
    code.len() <= 5
        && (canonical_market_hint(market) == "HK"
            || currency.trim().eq_ignore_ascii_case("HKD")
            || code.len() <= 4)
}

fn chinese_lookup_variants(value: &str) -> Vec<String> {
    let mut variants = Vec::new();
    push_unique_variant(&mut variants, value.trim().to_string());
    push_unique_variant(&mut variants, simplified_chinese(value));
    push_unique_variant(&mut variants, hong_kong_traditional_chinese(value));
    variants
}

fn push_unique_variant(variants: &mut Vec<String>, value: String) {
    let value = value.trim().to_string();
    if !value.is_empty() && !variants.iter().any(|variant| variant == &value) {
        variants.push(value);
    }
}

fn simplified_chinese(value: &str) -> String {
    static T2S_CONVERTER: OnceLock<Mutex<zhhz::Converter>> = OnceLock::new();
    T2S_CONVERTER
        .get_or_init(|| Mutex::new(zhhz::Converter::new(zhhz::Config::T2s)))
        .lock()
        .map(|converter| converter.convert(value))
        .unwrap_or_else(|_| value.to_string())
}

fn hong_kong_traditional_chinese(value: &str) -> String {
    static S2HK_CONVERTER: OnceLock<Mutex<zhhz::Converter>> = OnceLock::new();
    S2HK_CONVERTER
        .get_or_init(|| Mutex::new(zhhz::Converter::new(zhhz::Config::S2hk)))
        .lock()
        .map(|converter| converter.convert(value))
        .unwrap_or_else(|_| value.to_string())
}

fn contains_cjk(value: &str) -> bool {
    value
        .chars()
        .any(|character| ('\u{4e00}'..='\u{9fff}').contains(&character))
}

struct TushareSymbolDirectoryProvider {
    client: Client,
    token: String,
}

impl TushareSymbolDirectoryProvider {
    fn new(token: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent("Prudentia/0.1")
            .build()
            .unwrap_or_else(|_| Client::new());
        Self { client, token }
    }

    async fn refresh(&self, pool: &SqlitePool) -> AppResult<SecuritySymbolRefreshResult> {
        let mut all_symbols = Vec::new();
        let mut errors = Vec::new();
        let mut skipped_count = 0;

        for request in tushare_symbol_requests() {
            match self.fetch_symbols(&request).await {
                Ok(mut symbols) => {
                    skipped_count += symbols.iter().filter(|symbol| symbol.symbol.is_empty()).count();
                    all_symbols.append(&mut symbols);
                }
                Err(error) => {
                    tracing::warn!(
                        api_name = request.api_name,
                        error = %error,
                        "Tushare symbol directory fetch failed"
                    );
                    errors.push(format!("{}: {}", request.api_name, error));
                }
            }
        }

        all_symbols.retain(|symbol| !symbol.symbol.is_empty() && !symbol.name.is_empty());
        let upserted_count = upsert_security_symbols(pool, &all_symbols).await?;
        tracing::info!(
            upserted_count,
            skipped_count,
            error_count = errors.len(),
            "security symbol directory refreshed"
        );

        Ok(SecuritySymbolRefreshResult {
            provider: "tushare".to_string(),
            upserted_count,
            skipped_count,
            errors,
        })
    }

    async fn fetch_symbols(
        &self,
        request: &TushareSymbolRequest,
    ) -> AppResult<Vec<SecuritySymbol>> {
        let response = self
            .client
            .post(TUSHARE_API_URL)
            .json(&TushareRequest {
                api_name: request.api_name,
                token: &self.token,
                params: request.params.clone(),
                fields: request.fields,
            })
            .send()
            .await
            .map_err(|error| AppError::bad_request(error.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            return Err(AppError::bad_request(format!(
                "Tushare returned HTTP {}",
                status.as_u16()
            )));
        }

        let response = response
            .json::<TushareResponse>()
            .await
            .map_err(|error| AppError::bad_request(error.to_string()))?;
        if response.code != 0 {
            return Err(AppError::bad_request(
                response
                    .msg
                    .unwrap_or_else(|| format!("Tushare returned code {}", response.code)),
            ));
        }

        let data = response
            .data
            .ok_or_else(|| AppError::bad_request("Tushare response missing data"))?;
        Ok(tushare_symbols_from_data(data, request))
    }
}

#[derive(Clone)]
struct TushareSymbolRequest {
    api_name: &'static str,
    params: serde_json::Value,
    fields: &'static str,
    asset_type: &'static str,
}

fn tushare_symbol_requests() -> Vec<TushareSymbolRequest> {
    vec![
        TushareSymbolRequest {
            api_name: "stock_basic",
            params: serde_json::json!({ "list_status": "L" }),
            fields: "ts_code,name,market,exchange,list_status",
            asset_type: "stock",
        },
        TushareSymbolRequest {
            api_name: "fund_basic",
            params: serde_json::json!({ "market": "E", "status": "L" }),
            fields: "ts_code,name,market,status",
            asset_type: "fund",
        },
        TushareSymbolRequest {
            api_name: "hk_basic",
            params: serde_json::json!({}),
            fields: "ts_code,name,fullname,market,list_status",
            asset_type: "stock",
        },
    ]
}

fn tushare_symbols_from_data(
    data: TushareData,
    request: &TushareSymbolRequest,
) -> Vec<SecuritySymbol> {
    data.items
        .into_iter()
        .filter_map(|item| {
            let values = data
                .fields
                .iter()
                .cloned()
                .zip(item.into_iter().map(json_value_to_string))
                .collect::<HashMap<_, _>>();
            let ts_code = values.get("ts_code")?.trim();
            let name = values.get("name")?.trim();
            let symbol = internal_symbol_from_tushare_code(ts_code);
            let market = infer_market(&symbol).unwrap_or_else(|| "Other".to_string());
            let currency = inferred_currency(&symbol, Some(&market)).unwrap_or_default();
            Some(SecuritySymbol {
                symbol,
                name: name.to_string(),
                market,
                currency,
                asset_type: request.asset_type.to_string(),
                exchange: values.get("exchange").cloned().or_else(|| values.get("market").cloned()),
                provider: "tushare".to_string(),
                updated_at: now_iso(),
            })
        })
        .collect()
}

fn json_value_to_string(value: serde_json::Value) -> String {
    match value {
        serde_json::Value::String(value) => value,
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::Bool(value) => value.to_string(),
        _ => String::new(),
    }
}

fn internal_symbol_from_tushare_code(ts_code: &str) -> String {
    let upper = ts_code.trim().to_ascii_uppercase();
    if let Some(code) = upper.strip_suffix(".SH") {
        return format!("{code}.SS");
    }
    if let Some(code) = upper.strip_suffix(".HK") {
        return format!("{}.HK", normalize_hk_numeric_code(code));
    }
    upper
}

fn normalize_hk_numeric_code(code: &str) -> String {
    let stripped = code.trim_start_matches('0');
    let stripped = if stripped.is_empty() { "0" } else { stripped };
    if stripped.len() < 4 {
        format!("{stripped:0>4}")
    } else {
        stripped.to_string()
    }
}
