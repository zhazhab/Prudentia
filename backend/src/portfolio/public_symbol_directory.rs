struct PublicSymbolDirectoryProvider {
    client: Client,
}

impl PublicSymbolDirectoryProvider {
    fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("Prudentia/0.1")
            .build()
            .unwrap_or_else(|_| Client::new());
        Self { client }
    }

    async fn refresh(&self, pool: &SqlitePool) -> AppResult<SecuritySymbolRefreshResult> {
        let config = load_public_symbol_directory_config()?;
        let inventory_path = public_symbol_inventory_path(&config);
        let ttl = public_symbol_cache_ttl(&config);
        let inventory = match read_public_symbol_inventory(&inventory_path) {
            Ok(inventory) => {
                tracing::info!(
                    inventory_path = %inventory_path.display(),
                    symbol_count = inventory.symbols.len(),
                    updated_at = %inventory.updated_at,
                    "public security symbol inventory loaded"
                );
                Some(inventory)
            }
            Err(error) => {
                tracing::warn!(
                    inventory_path = %inventory_path.display(),
                    error = %error,
                    "public security symbol inventory unavailable"
                );
                None
            }
        };

        if let Some(inventory) = inventory.as_ref() {
            let inventory_symbols = public_symbol_inventory_security_symbols(inventory);
            let upserted_count = upsert_security_symbols(pool, &inventory_symbols).await?;
            if !public_symbol_inventory_is_expired(inventory, ttl) {
                tracing::info!(
                    inventory_path = %inventory_path.display(),
                    upserted_count,
                    "public security symbol inventory fresh; source refresh skipped"
                );
                return Ok(SecuritySymbolRefreshResult {
                    provider: "public".to_string(),
                    upserted_count,
                    skipped_count: 0,
                    errors: Vec::new(),
                });
            }

            tracing::info!(
                inventory_path = %inventory_path.display(),
                updated_at = %inventory.updated_at,
                ttl_secs = ttl.as_secs(),
                "public security symbol inventory expired; refreshing sources"
            );
        }

        let refreshed_inventory = self.fetch_source_inventory(&config).await?;
        let errors = refreshed_inventory.errors;
        if !errors.is_empty() {
            tracing::warn!(
                error_count = errors.len(),
                "public security symbol source refresh had errors"
            );
            return Ok(SecuritySymbolRefreshResult {
                provider: "public".to_string(),
                upserted_count: inventory
                    .as_ref()
                    .map(|inventory| inventory.symbols.len())
                    .unwrap_or(0),
                skipped_count: 0,
                errors,
            });
        }

        write_public_symbol_inventory(&inventory_path, &refreshed_inventory.inventory)?;
        let inventory_symbols =
            public_symbol_inventory_security_symbols(&refreshed_inventory.inventory);
        let upserted_count = upsert_security_symbols(pool, &inventory_symbols).await?;
        tracing::info!(
            upserted_count,
            source_count = refreshed_inventory.inventory.sources.len(),
            inventory_path = %inventory_path.display(),
            "public security symbol directory refreshed"
        );

        Ok(SecuritySymbolRefreshResult {
            provider: "public".to_string(),
            upserted_count,
            skipped_count: 0,
            errors: Vec::new(),
        })
    }

    async fn fetch_source_inventory(
        &self,
        config: &PublicSymbolDirectoryConfig,
    ) -> AppResult<PublicSymbolInventoryRefresh> {
        let mut all_symbols = Vec::new();
        let mut source_updates = Vec::new();
        let mut errors = Vec::new();

        for source in &config.sources {
            match self.fetch_source_symbols(config, source).await {
                Ok(mut symbols) => {
                    source_updates.push(PublicSymbolInventorySource {
                        id: source.id.clone(),
                        count: symbols.len(),
                    });
                    all_symbols.append(&mut symbols);
                }
                Err(error) => errors.push(format!("{}: {}", source.id, error)),
            }
        }

        Ok(PublicSymbolInventoryRefresh {
            inventory: PublicSymbolInventory {
                schema_version: 1,
                updated_at: now_iso(),
                sources: source_updates,
                symbols: normalize_public_symbol_inventory_symbols(all_symbols),
            },
            errors,
        })
    }

    async fn fetch_source_symbols(
        &self,
        config: &PublicSymbolDirectoryConfig,
        source: &PublicSymbolDirectorySource,
    ) -> AppResult<Vec<SecuritySymbol>> {
        let bytes = self.fetch_cached_bytes(config, source).await?;
        let provider = source
            .provider
            .clone()
            .unwrap_or_else(|| format!("public:{}", source.id));

        let mut symbols = match source.kind.as_str() {
            "hkex" => hkex_symbols_from_xlsx_bytes(bytes)?,
            "nasdaq_listed" => {
                let text = bytes_to_public_symbol_text(bytes)?;
                nasdaq_symbols_from_listed_text(&text)
            }
            "nasdaq_other_listed" => {
                let text = bytes_to_public_symbol_text(bytes)?;
                nasdaq_symbols_from_other_listed_text(&text)
            }
            "sse_suggest" => {
                let text = bytes_to_public_symbol_text(bytes)?;
                sse_symbols_from_suggest_text(
                    &text,
                    source.asset_type.as_deref().unwrap_or("security"),
                )
            }
            other => {
                return Err(AppError::bad_request(format!(
                    "unsupported public symbol source kind '{other}'"
                )));
            }
        };

        for symbol in &mut symbols {
            symbol.provider = provider.clone();
        }
        Ok(symbols)
    }

    async fn fetch_cached_bytes(
        &self,
        config: &PublicSymbolDirectoryConfig,
        source: &PublicSymbolDirectorySource,
    ) -> AppResult<Vec<u8>> {
        let cache_path = public_symbol_cache_path(config, source);
        let ttl = public_symbol_cache_ttl(config);
        if let Some(bytes) = read_fresh_public_symbol_cache(&cache_path, ttl)? {
            tracing::info!(
                url = %source.url,
                cache_path = %cache_path.display(),
                "public symbol source cache hit"
            );
            return Ok(bytes);
        }

        match self.download_bytes(&source.url).await {
            Ok(bytes) => {
                write_public_symbol_cache(&cache_path, &bytes, ttl);
                Ok(bytes)
            }
            Err(error) => match fs::read(&cache_path) {
                Ok(bytes) => {
                    tracing::warn!(
                        url = %source.url,
                        cache_path = %cache_path.display(),
                        error = %error,
                        "using stale public symbol source cache after fetch failed"
                    );
                    Ok(bytes)
                }
                Err(_) => Err(error),
            },
        }
    }

    async fn download_bytes(&self, url: &str) -> AppResult<Vec<u8>> {
        self.client
            .get(url)
            .send()
            .await
            .map_err(|error| AppError::bad_request(error.to_string()))?
            .error_for_status()
            .map_err(|error| AppError::bad_request(error.to_string()))?
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|error| AppError::bad_request(error.to_string()))
    }
}

#[derive(Debug, Clone, Deserialize)]
struct PublicSymbolDirectoryConfig {
    cache_dir: String,
    #[serde(default = "default_public_symbol_inventory_file")]
    inventory_file: String,
    cache_ttl_secs: Option<u64>,
    sources: Vec<PublicSymbolDirectorySource>,
}

#[derive(Debug, Clone, Deserialize)]
struct PublicSymbolDirectorySource {
    id: String,
    kind: String,
    url: String,
    cache_file: String,
    provider: Option<String>,
    asset_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PublicSymbolInventory {
    schema_version: u32,
    updated_at: String,
    sources: Vec<PublicSymbolInventorySource>,
    symbols: Vec<PublicSymbolInventorySymbol>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PublicSymbolInventorySource {
    id: String,
    count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PublicSymbolInventorySymbol {
    symbol: String,
    name: String,
    market: String,
    currency: String,
}

struct PublicSymbolInventoryRefresh {
    inventory: PublicSymbolInventory,
    errors: Vec<String>,
}

fn load_public_symbol_directory_config() -> AppResult<PublicSymbolDirectoryConfig> {
    let path = public_symbol_directory_config_path();
    let text = fs::read_to_string(&path).map_err(|error| {
        AppError::bad_request(format!(
            "failed to read public symbol directory config {}: {}",
            path.display(),
            error
        ))
    })?;
    let mut config =
        serde_json::from_str::<PublicSymbolDirectoryConfig>(&text).map_err(AppError::from)?;
    let project_root = public_symbol_directory_config_root(&path);
    if PathBuf::from(&config.cache_dir).is_relative() {
        config.cache_dir = project_root
            .join(&config.cache_dir)
            .to_string_lossy()
            .to_string();
    }
    if PathBuf::from(&config.inventory_file).is_relative() {
        config.inventory_file = project_root
            .join(&config.inventory_file)
            .to_string_lossy()
            .to_string();
    }
    Ok(config)
}

fn default_public_symbol_inventory_file() -> String {
    "data/symbol-directory/public/symbols.json".to_string()
}

fn public_symbol_directory_config_path() -> PathBuf {
    if let Ok(path) = std::env::var("SYMBOL_DIRECTORY_PUBLIC_CONFIG") {
        let configured_path = PathBuf::from(path);
        if configured_path.is_absolute() || configured_path.exists() {
            return configured_path;
        }

        let manifest_relative = public_symbol_manifest_project_root().join(&configured_path);
        if manifest_relative.exists() {
            return manifest_relative;
        }

        return configured_path;
    }

    let cwd_relative = PathBuf::from("config").join("symbol-directory-public.json");
    if cwd_relative.exists() {
        return cwd_relative;
    }

    public_symbol_manifest_project_root()
        .join("config")
        .join("symbol-directory-public.json")
}

fn public_symbol_manifest_project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
}

fn public_symbol_directory_config_root(path: &FsPath) -> PathBuf {
    let Some(parent) = path.parent() else {
        return PathBuf::from(".");
    };
    if parent.file_name().and_then(|name| name.to_str()) == Some("config") {
        return parent
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| parent.to_path_buf());
    }
    parent.to_path_buf()
}

fn public_symbol_cache_path(
    config: &PublicSymbolDirectoryConfig,
    source: &PublicSymbolDirectorySource,
) -> PathBuf {
    PathBuf::from(&config.cache_dir).join(&source.cache_file)
}

fn public_symbol_inventory_path(config: &PublicSymbolDirectoryConfig) -> PathBuf {
    PathBuf::from(&config.inventory_file)
}

fn public_symbol_cache_ttl(config: &PublicSymbolDirectoryConfig) -> Duration {
    Duration::from_secs(config.cache_ttl_secs.unwrap_or(24 * 60 * 60).max(1))
}

fn read_public_symbol_inventory(inventory_path: &FsPath) -> AppResult<PublicSymbolInventory> {
    let text = fs::read_to_string(inventory_path).map_err(|error| {
        AppError::bad_request(format!(
            "failed to read public symbol inventory {}: {}",
            inventory_path.display(),
            error
        ))
    })?;
    let inventory = serde_json::from_str::<PublicSymbolInventory>(&text).map_err(AppError::from)?;
    if inventory.schema_version != 1 {
        return Err(AppError::bad_request(format!(
            "unsupported public symbol inventory schema_version {}",
            inventory.schema_version
        )));
    }
    Ok(inventory)
}

fn write_public_symbol_inventory(
    inventory_path: &FsPath,
    inventory: &PublicSymbolInventory,
) -> AppResult<()> {
    if let Some(parent) = inventory_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            AppError::bad_request(format!(
                "failed to create public symbol inventory directory {}: {}",
                parent.display(),
                error
            ))
        })?;
    }

    let bytes = serde_json::to_vec_pretty(inventory).map_err(AppError::from)?;
    let temp_path = inventory_path.with_extension("json.tmp");
    fs::write(&temp_path, &bytes).map_err(|error| {
        AppError::bad_request(format!(
            "failed to write public symbol inventory {}: {}",
            temp_path.display(),
            error
        ))
    })?;
    fs::rename(&temp_path, inventory_path).map_err(|error| {
        AppError::bad_request(format!(
            "failed to replace public symbol inventory {}: {}",
            inventory_path.display(),
            error
        ))
    })?;

    tracing::info!(
        inventory_path = %inventory_path.display(),
        bytes = bytes.len(),
        symbol_count = inventory.symbols.len(),
        source_count = inventory.sources.len(),
        updated_at = %inventory.updated_at,
        "public security symbol inventory written"
    );
    Ok(())
}

fn public_symbol_inventory_is_expired(
    inventory: &PublicSymbolInventory,
    ttl: Duration,
) -> bool {
    let Ok(updated_at) = chrono::DateTime::parse_from_rfc3339(&inventory.updated_at) else {
        return true;
    };
    let Ok(ttl) = chrono::Duration::from_std(ttl) else {
        return true;
    };
    chrono::Utc::now().signed_duration_since(updated_at) > ttl
}

fn public_symbol_inventory_security_symbols(
    inventory: &PublicSymbolInventory,
) -> Vec<SecuritySymbol> {
    inventory
        .symbols
        .iter()
        .map(|symbol| SecuritySymbol {
            symbol: symbol.symbol.clone(),
            name: symbol.name.clone(),
            market: symbol.market.clone(),
            currency: symbol.currency.clone(),
            asset_type: "security".to_string(),
            exchange: None,
            provider: "public:inventory".to_string(),
            updated_at: inventory.updated_at.clone(),
        })
        .collect()
}

fn normalize_public_symbol_inventory_symbols(
    symbols: Vec<SecuritySymbol>,
) -> Vec<PublicSymbolInventorySymbol> {
    let mut by_symbol = HashMap::new();
    for symbol in symbols {
        if symbol.symbol.trim().is_empty() || symbol.name.trim().is_empty() {
            continue;
        }
        by_symbol.insert(
            symbol.symbol.clone(),
            PublicSymbolInventorySymbol {
                symbol: symbol.symbol,
                name: simplified_chinese(&symbol.name),
                market: symbol.market,
                currency: symbol.currency,
            },
        );
    }

    let mut symbols = by_symbol.into_values().collect::<Vec<_>>();
    symbols.sort_by(|left, right| left.symbol.cmp(&right.symbol));
    symbols
}

fn read_fresh_public_symbol_cache(
    cache_path: &FsPath,
    ttl: Duration,
) -> AppResult<Option<Vec<u8>>> {
    let Ok(metadata) = fs::metadata(cache_path) else {
        return Ok(None);
    };
    let modified = metadata
        .modified()
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    if modified.elapsed().unwrap_or(ttl + Duration::from_secs(1)) > ttl {
        return Ok(None);
    }
    fs::read(cache_path)
        .map(Some)
        .map_err(|error| AppError::bad_request(error.to_string()))
}

fn write_public_symbol_cache(cache_path: &FsPath, bytes: &[u8], ttl: Duration) {
    if let Some(parent) = cache_path.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            tracing::warn!(
                cache_path = %cache_path.display(),
                error = %error,
                "failed to create public symbol source cache directory"
            );
            return;
        }
    }

    if let Err(error) = fs::write(cache_path, bytes) {
        tracing::warn!(
            cache_path = %cache_path.display(),
            error = %error,
            "failed to write public symbol source cache"
        );
        return;
    }

    tracing::info!(
        cache_path = %cache_path.display(),
        bytes = bytes.len(),
        ttl_secs = ttl.as_secs(),
        "public symbol source cache written"
    );
}

fn bytes_to_public_symbol_text(bytes: Vec<u8>) -> AppResult<String> {
    String::from_utf8(bytes).map_err(|error| AppError::bad_request(error.to_string()))
}

fn hkex_symbols_from_xlsx_bytes(bytes: Vec<u8>) -> AppResult<Vec<SecuritySymbol>> {
    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|err| AppError::bad_request(err.to_string()))?;
    let mut sheet_xml = String::new();
    archive
        .by_name("xl/worksheets/sheet1.xml")
        .map_err(|err| AppError::bad_request(err.to_string()))?
        .read_to_string(&mut sheet_xml)
        .map_err(|err| AppError::bad_request(err.to_string()))?;
    Ok(hkex_symbols_from_rows(&hkex_rows_from_sheet_xml(
        &sheet_xml,
    )))
}

fn hkex_rows_from_sheet_xml(sheet_xml: &str) -> Vec<Vec<String>> {
    sheet_xml
        .split("<x:row ")
        .skip(1)
        .filter_map(|row_part| {
            let row_xml = row_part.split_once("</x:row>")?.0;
            let mut row = vec![String::new(); 17];
            row[0] = xlsx_inline_cell_value(row_xml, "A").unwrap_or_default();
            row[1] = xlsx_inline_cell_value(row_xml, "B").unwrap_or_default();
            row[2] = xlsx_inline_cell_value(row_xml, "C").unwrap_or_default();
            row[16] = xlsx_inline_cell_value(row_xml, "Q").unwrap_or_default();
            Some(row)
        })
        .collect()
}

fn xlsx_inline_cell_value(row_xml: &str, column: &str) -> Option<String> {
    let cell_marker = format!(r#"<x:c r="{column}"#);
    let cell_xml = row_xml.split(&cell_marker).nth(1)?;
    let cell_xml = cell_xml
        .split_once("</x:c>")
        .map(|(value, _)| value)
        .unwrap_or(cell_xml);
    let value_xml = cell_xml.split_once("<x:v>")?.1.split_once("</x:v>")?.0;
    Some(unescape_basic_xml(value_xml).trim().to_string())
}

fn unescape_basic_xml(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn hkex_symbols_from_rows(rows: &[Vec<String>]) -> Vec<SecuritySymbol> {
    let header_index = rows
        .iter()
        .position(|row| {
            row.first().is_some_and(|value| {
                matches!(value.trim(), "Stock Code" | "股份代號" | "股份代号")
            })
        })
        .unwrap_or(0);

    rows.iter()
        .skip(header_index + 1)
        .filter_map(|row| {
            let code = row.first()?.trim();
            let name = row.get(1)?.trim();
            if code.is_empty() || name.is_empty() || !code.chars().all(|value| value.is_ascii_digit())
            {
                return None;
            }
            let category = row.get(2).map(String::as_str).unwrap_or_default();
            let currency = row.get(16).map(String::as_str).unwrap_or("HKD").trim();
            Some(SecuritySymbol {
                symbol: format!("{}.HK", normalize_hk_numeric_code(code)),
                name: clean_public_security_name(name),
                market: "HK".to_string(),
                currency: if currency.is_empty() {
                    "HKD".to_string()
                } else {
                    currency.to_ascii_uppercase()
                },
                asset_type: public_asset_type(category, name),
                exchange: Some("HKEX".to_string()),
                provider: "public:hkex".to_string(),
                updated_at: now_iso(),
            })
        })
        .collect()
}

fn nasdaq_symbols_from_listed_text(text: &str) -> Vec<SecuritySymbol> {
    let mut lines = text.lines();
    let headers = lines.next().unwrap_or_default().split('|').collect::<Vec<_>>();
    let symbol_index = header_index(&headers, "Symbol").unwrap_or(0);
    let name_index = header_index(&headers, "Security Name").unwrap_or(1);
    let test_index = header_index(&headers, "Test Issue");
    let etf_index = header_index(&headers, "ETF");

    lines
        .filter_map(|line| {
            let values = line.split('|').collect::<Vec<_>>();
            if values.len() <= name_index
                || values
                    .first()
                    .is_some_and(|value| *value == "File Creation Time")
            {
                return None;
            }
            if test_index
                .and_then(|index| values.get(index))
                .is_some_and(|value| value.eq_ignore_ascii_case("Y"))
            {
                return None;
            }
            security_symbol_from_us_fields(
                values.get(symbol_index).copied().unwrap_or_default(),
                values.get(name_index).copied().unwrap_or_default(),
                etf_index
                    .and_then(|index| values.get(index))
                    .copied()
                    .unwrap_or_default(),
                "NASDAQ",
            )
        })
        .collect()
}

fn nasdaq_symbols_from_other_listed_text(text: &str) -> Vec<SecuritySymbol> {
    let mut lines = text.lines();
    let headers = lines.next().unwrap_or_default().split('|').collect::<Vec<_>>();
    let symbol_index = header_index(&headers, "ACT Symbol").unwrap_or(0);
    let name_index = header_index(&headers, "Security Name").unwrap_or(1);
    let exchange_index = header_index(&headers, "Exchange").unwrap_or(2);
    let etf_index = header_index(&headers, "ETF");
    let test_index = header_index(&headers, "Test Issue");

    lines
        .filter_map(|line| {
            let values = line.split('|').collect::<Vec<_>>();
            if values.len() <= name_index
                || values
                    .first()
                    .is_some_and(|value| *value == "File Creation Time")
            {
                return None;
            }
            if test_index
                .and_then(|index| values.get(index))
                .is_some_and(|value| value.eq_ignore_ascii_case("Y"))
            {
                return None;
            }
            security_symbol_from_us_fields(
                values.get(symbol_index).copied().unwrap_or_default(),
                values.get(name_index).copied().unwrap_or_default(),
                etf_index
                    .and_then(|index| values.get(index))
                    .copied()
                    .unwrap_or_default(),
                us_exchange_name(values.get(exchange_index).copied().unwrap_or_default()),
            )
        })
        .collect()
}

fn security_symbol_from_us_fields(
    symbol: &str,
    name: &str,
    etf: &str,
    exchange: &str,
) -> Option<SecuritySymbol> {
    let symbol = symbol.trim();
    let name = name.trim();
    if symbol.is_empty() || name.is_empty() {
        return None;
    }
    Some(SecuritySymbol {
        symbol: symbol.to_ascii_uppercase(),
        name: clean_public_security_name(name),
        market: "US".to_string(),
        currency: "USD".to_string(),
        asset_type: if etf.eq_ignore_ascii_case("Y") {
            "fund".to_string()
        } else {
            "stock".to_string()
        },
        exchange: Some(exchange.to_string()),
        provider: "public:nasdaq".to_string(),
        updated_at: now_iso(),
    })
}

fn sse_symbols_from_suggest_text(text: &str, asset_type: &str) -> Vec<SecuritySymbol> {
    text.split("_t.push({")
        .skip(1)
        .filter_map(|entry| {
            let code = quoted_object_value(entry, "val")?;
            let name = quoted_object_value(entry, "val2")?;
            if code.is_empty() || name.is_empty() {
                return None;
            }
            Some(SecuritySymbol {
                symbol: format!("{code}.SS"),
                name,
                market: "CN".to_string(),
                currency: "CNY".to_string(),
                asset_type: asset_type.to_string(),
                exchange: Some("SSE".to_string()),
                provider: "public:sse".to_string(),
                updated_at: now_iso(),
            })
        })
        .collect()
}

fn quoted_object_value(entry: &str, key: &str) -> Option<String> {
    let marker = format!(r#"{key}:""#);
    let start = entry.find(&marker)? + marker.len();
    let rest = &entry[start..];
    let end = rest.find('"')?;
    Some(rest[..end].trim().to_string())
}

fn header_index(headers: &[&str], name: &str) -> Option<usize> {
    headers
        .iter()
        .position(|header| header.trim().eq_ignore_ascii_case(name))
}

fn public_asset_type(category: &str, name: &str) -> String {
    let normalized = format!("{category} {name}").to_ascii_lowercase();
    if normalized.contains("fund") || normalized.contains("etf") || normalized.contains("基金") {
        "fund".to_string()
    } else if normalized.contains("equity")
        || normalized.contains("stock")
        || normalized.contains("股份")
        || normalized.contains("股本")
        || normalized.contains("股票")
    {
        "stock".to_string()
    } else {
        "security".to_string()
    }
}

fn clean_public_security_name(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn us_exchange_name(value: &str) -> &'static str {
    match value.trim() {
        "A" => "NYSE American",
        "N" => "NYSE",
        "P" => "NYSE Arca",
        "Z" => "Cboe BZX",
        "V" => "IEX",
        _ => "US",
    }
}
