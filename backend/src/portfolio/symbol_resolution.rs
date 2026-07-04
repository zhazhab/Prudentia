#[async_trait]
trait PortfolioSymbolResolver: Send + Sync {
    async fn resolve_symbol(
        &self,
        company_name: &str,
        market: &str,
        currency: &str,
    ) -> AppResult<Option<String>>;
}

struct LocalSymbolDirectoryResolver {
    pool: SqlitePool,
}

#[derive(Debug, Clone)]
struct ExistingPositionSymbolMatch {
    symbol: String,
}

impl LocalSymbolDirectoryResolver {
    fn new(pool: &SqlitePool) -> Self {
        Self { pool: pool.clone() }
    }
}

#[async_trait]
impl PortfolioSymbolResolver for LocalSymbolDirectoryResolver {
    async fn resolve_symbol(
        &self,
        company_name: &str,
        market: &str,
        currency: &str,
    ) -> AppResult<Option<String>> {
        Ok(resolve_security_symbol(&self.pool, company_name, market, currency)
            .await?
            .map(|symbol| symbol.symbol))
    }
}

fn resolve_symbol_from_existing_positions(
    positions: &[PortfolioPosition],
    company_name: &str,
    market: &str,
    currency: &str,
) -> Option<ExistingPositionSymbolMatch> {
    let normalized_name = normalize_symbol_lookup_text(company_name);
    if normalized_name.is_empty() {
        return None;
    }

    let normalized_market = normalize_market(market);
    let normalized_currency = normalize_currency_code(currency);
    let mut matches = positions.iter().filter(|position| {
        existing_position_matches_symbol_query(
            position,
            &normalized_name,
            &normalized_market,
            &normalized_currency,
        )
    });

    let first = matches.next()?;
    matches.next().is_none().then(|| ExistingPositionSymbolMatch {
        symbol: first.symbol.clone(),
    })
}

fn existing_position_matches_symbol_query(
    position: &PortfolioPosition,
    normalized_name: &str,
    normalized_market: &str,
    normalized_currency: &str,
) -> bool {
    if normalize_symbol_lookup_text(&position.name) != normalized_name {
        return false;
    }

    let position_market = position
        .market
        .as_deref()
        .map(normalize_market)
        .unwrap_or_default();
    if !normalized_market.is_empty() && position_market != normalized_market {
        return false;
    }

    let position_currency = normalize_currency_code(&position.currency);
    if !normalized_currency.is_empty() && position_currency != normalized_currency {
        return false;
    }

    true
}

fn apply_resolved_symbol_to_draft_row(row: &mut PortfolioDraftRow, symbol: &str) {
    row.symbol = symbol.to_ascii_uppercase();
    row.market = infer_market(&row.symbol).unwrap_or_else(|| row.market.clone());
    if row.currency.trim().is_empty() {
        if let Some(currency) = inferred_currency(&row.symbol, Some(&row.market)) {
            row.currency = currency;
        }
    }
    *row = normalize_draft_row(row.clone());
}
