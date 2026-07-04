#[cfg(test)]
mod tests {
    use super::*;
    use crate::{database, market_data::mock::MockMarketDataProvider};
    use async_trait::async_trait;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::{collections::VecDeque, sync::Mutex};

    struct FixedSymbolResolver {
        symbol: Option<String>,
    }

    #[async_trait]
    impl PortfolioSymbolResolver for FixedSymbolResolver {
        async fn resolve_symbol(
            &self,
            _company_name: &str,
            _market: &str,
            _currency: &str,
        ) -> AppResult<Option<String>> {
            Ok(self.symbol.clone())
        }
    }

    struct SequentialSymbolResolver {
        symbols: Mutex<VecDeque<Option<String>>>,
    }

    #[async_trait]
    impl PortfolioSymbolResolver for SequentialSymbolResolver {
        async fn resolve_symbol(
            &self,
            _company_name: &str,
            _market: &str,
            _currency: &str,
        ) -> AppResult<Option<String>> {
            Ok(self
                .symbols
                .lock()
                .expect("symbol resolver lock")
                .pop_front()
                .unwrap_or(None))
        }
    }

    #[tokio::test]
    async fn security_symbols_schema_stores_only_lookup_fields() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect sqlite");
        database::migrate(&pool).await.expect("migrate");

        let columns = sqlx::query("PRAGMA table_info(security_symbols);")
            .fetch_all(&pool)
            .await
            .expect("table info")
            .into_iter()
            .map(|row| row.get::<String, _>("name"))
            .collect::<Vec<_>>();

        assert_eq!(
            columns,
            vec!["symbol", "name", "market", "currency", "updated_at"]
        );
    }

    #[test]
    fn security_symbol_serializes_only_lookup_fields() {
        let json = serde_json::to_value(security_symbol(
            "0700.HK",
            "腾讯控股",
            "HK",
            "HKD",
            "stock",
        ))
        .expect("serialize symbol");

        assert_eq!(
            json.as_object()
                .expect("symbol object")
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            vec!["currency", "market", "name", "symbol"]
        );
    }

    #[tokio::test]
    async fn draft_commit_resolves_missing_symbol_from_company_name() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect sqlite");
        database::migrate(&pool).await.expect("migrate");

        commit_draft_rows_with_symbol_resolver(
            &pool,
            Arc::new(MockMarketDataProvider),
            PortfolioDraftCommitRequest {
                rows: vec![PortfolioDraftRow {
                    symbol: "".to_string(),
                    name: "Tencent".to_string(),
                    quantity: "100".to_string(),
                    average_cost: "300".to_string(),
                    currency: "HKD".to_string(),
                    account: None,
                    market: "HK".to_string(),
                    sector: None,
                    imported_market_value: Some("32000".to_string()),
                    last_price: None,
                    notes: None,
                    confidence: "high".to_string(),
                    warnings: Vec::new(),
                    errors: Vec::new(),
                }],
            },
            &FixedSymbolResolver {
                symbol: Some("0700.HK".to_string()),
            },
        )
        .await
        .expect("commit");

        let positions = list_positions(&pool).await.expect("positions");
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].symbol, "0700.HK");
    }

    #[tokio::test]
    async fn draft_commit_reuses_symbol_from_unique_existing_position() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect sqlite");
        database::migrate(&pool).await.expect("migrate");

        commit_draft_rows_with_symbol_resolver(
            &pool,
            Arc::new(MockMarketDataProvider),
            PortfolioDraftCommitRequest {
                rows: vec![PortfolioDraftRow {
                    symbol: "510300".to_string(),
                    name: "300ETF".to_string(),
                    quantity: "100".to_string(),
                    average_cost: "4.729".to_string(),
                    currency: "CNY".to_string(),
                    account: None,
                    market: "CN".to_string(),
                    sector: None,
                    imported_market_value: None,
                    last_price: Some("4.850".to_string()),
                    notes: None,
                    confidence: "high".to_string(),
                    warnings: Vec::new(),
                    errors: Vec::new(),
                }],
            },
            &FixedSymbolResolver { symbol: None },
        )
        .await
        .expect("seed position");

        commit_draft_rows_with_symbol_resolver(
            &pool,
            Arc::new(MockMarketDataProvider),
            PortfolioDraftCommitRequest {
                rows: vec![PortfolioDraftRow {
                    symbol: "".to_string(),
                    name: "300ETF".to_string(),
                    quantity: "3300".to_string(),
                    average_cost: "4.729".to_string(),
                    currency: "CNY".to_string(),
                    account: None,
                    market: "CN".to_string(),
                    sector: None,
                    imported_market_value: Some("16005.00".to_string()),
                    last_price: Some("4.850".to_string()),
                    notes: None,
                    confidence: "high".to_string(),
                    warnings: Vec::new(),
                    errors: Vec::new(),
                }],
            },
            &FixedSymbolResolver { symbol: None },
        )
        .await
        .expect("commit reuses existing symbol");

        let positions = list_positions(&pool).await.expect("positions");
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].symbol, "510300");
        assert_eq!(positions[0].name, "300ETF");
        assert_eq!(positions[0].quantity, 3300.0);
        assert_eq!(positions[0].market.as_deref(), Some("CN"));
    }

    #[tokio::test]
    async fn draft_commit_requires_manual_symbol_when_lookup_cannot_resolve() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect sqlite");
        database::migrate(&pool).await.expect("migrate");

        let result = commit_draft_rows_with_symbol_resolver(
            &pool,
            Arc::new(MockMarketDataProvider),
            PortfolioDraftCommitRequest {
                rows: vec![PortfolioDraftRow {
                    symbol: "".to_string(),
                    name: "Broker Short Name".to_string(),
                    quantity: "100".to_string(),
                    average_cost: "10".to_string(),
                    currency: "CNY".to_string(),
                    account: None,
                    market: "CN".to_string(),
                    sector: None,
                    imported_market_value: Some("1000".to_string()),
                    last_price: None,
                    notes: None,
                    confidence: "high".to_string(),
                    warnings: Vec::new(),
                    errors: Vec::new(),
                }],
            },
            &FixedSymbolResolver { symbol: None },
        )
        .await;

        let error = result.expect_err("unresolved symbol blocks commit");
        assert!(error
            .to_string()
            .contains("symbol could not be resolved for company name Broker Short Name"));
        assert!(list_positions(&pool).await.expect("positions").is_empty());
    }

    #[tokio::test]
    async fn draft_commit_deduplicates_by_resolved_symbol_not_company_name() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect sqlite");
        database::migrate(&pool).await.expect("migrate");

        let row = PortfolioDraftRow {
            symbol: "".to_string(),
            name: "Tencent".to_string(),
            quantity: "100".to_string(),
            average_cost: "300".to_string(),
            currency: "HKD".to_string(),
            account: None,
            market: "HK".to_string(),
            sector: None,
            imported_market_value: Some("32000".to_string()),
            last_price: None,
            notes: None,
            confidence: "high".to_string(),
            warnings: Vec::new(),
            errors: Vec::new(),
        };

        commit_draft_rows_with_symbol_resolver(
            &pool,
            Arc::new(MockMarketDataProvider),
            PortfolioDraftCommitRequest {
                rows: vec![row.clone(), row],
            },
            &SequentialSymbolResolver {
                symbols: Mutex::new(VecDeque::from([
                    Some("0700.HK".to_string()),
                    Some("TCEHY".to_string()),
                ])),
            },
        )
        .await
        .expect("same company name with different resolved symbols can commit");

        let positions = list_positions(&pool).await.expect("positions");
        assert_eq!(positions.len(), 2);
        assert_eq!(positions[0].symbol, "0700.HK");
        assert_eq!(positions[1].symbol, "TCEHY");
    }

    #[tokio::test]
    async fn draft_commit_merges_duplicate_symbols() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect sqlite");
        database::migrate(&pool).await.expect("migrate");

        let first = PortfolioDraftRow {
            symbol: "0700.HK".to_string(),
            name: "Tencent".to_string(),
            quantity: "100".to_string(),
            average_cost: "300".to_string(),
            currency: "HKD".to_string(),
            account: Some("Longbridge".to_string()),
            market: "HK".to_string(),
            sector: None,
            imported_market_value: Some("32000".to_string()),
            last_price: None,
            notes: None,
            confidence: "high".to_string(),
            warnings: Vec::new(),
            errors: Vec::new(),
        };
        let second = PortfolioDraftRow {
            symbol: "0700.HK".to_string(),
            name: "腾讯控股".to_string(),
            quantity: "200".to_string(),
            average_cost: "400".to_string(),
            currency: "HKD".to_string(),
            account: Some("Tonghuashun".to_string()),
            market: "HK".to_string(),
            sector: None,
            imported_market_value: Some("84000".to_string()),
            last_price: None,
            notes: None,
            confidence: "high".to_string(),
            warnings: Vec::new(),
            errors: Vec::new(),
        };

        commit_draft_rows_with_symbol_resolver(
            &pool,
            Arc::new(MockMarketDataProvider),
            PortfolioDraftCommitRequest {
                rows: vec![first, second],
            },
            &FixedSymbolResolver { symbol: None },
        )
        .await
        .expect("duplicate symbols merge");

        let positions = list_positions(&pool).await.expect("positions");
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].symbol, "0700.HK");
        assert_eq!(positions[0].quantity, 300.0);
        assert!((positions[0].average_cost - 366.6666666667).abs() < 0.000001);
        assert_eq!(positions[0].market_value, 116000.0);
        assert_eq!(
            positions[0].account.as_deref(),
            Some("Longbridge, Tonghuashun")
        );
    }

    #[tokio::test]
    async fn draft_commit_prefers_visible_last_price_over_imported_market_value() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect sqlite");
        database::migrate(&pool).await.expect("migrate");

        commit_draft_rows_with_symbol_resolver(
            &pool,
            Arc::new(MockMarketDataProvider),
            PortfolioDraftCommitRequest {
                rows: vec![PortfolioDraftRow {
                    symbol: "0700.HK".to_string(),
                    name: "腾讯控股".to_string(),
                    quantity: "900".to_string(),
                    average_cost: "489.877".to_string(),
                    currency: "HKD".to_string(),
                    account: None,
                    market: "HK".to_string(),
                    sector: None,
                    imported_market_value: Some("335646.34".to_string()),
                    last_price: None,
                    notes: Some("available=900; current_price=430.200; pnl=-46560.22 (-12.182%)".to_string()),
                    confidence: "high".to_string(),
                    warnings: Vec::new(),
                    errors: Vec::new(),
                }],
            },
            &FixedSymbolResolver { symbol: None },
        )
        .await
        .expect("commit");

        let positions = list_positions(&pool).await.expect("positions");
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].last_price, Some(430.2));
        assert_eq!(positions[0].market_value, 387180.0);
    }

    #[tokio::test]
    async fn local_symbol_directory_resolves_with_market_hint() {
        let pool = migrated_pool().await;
        seed_security_symbols(
            &pool,
            vec![
                security_symbol("600036.SS", "招商银行", "CN", "CNY", "stock"),
                security_symbol("3968.HK", "招商银行", "HK", "HKD", "stock"),
            ],
        )
        .await;

        assert_eq!(
            resolve_security_symbol(&pool, "招商银行", "HK", "HKD")
                .await
                .expect("resolve")
                .map(|symbol| symbol.symbol),
            Some("3968.HK".to_string())
        );
        assert_eq!(
            resolve_security_symbol(&pool, "招商银行", "", "")
                .await
                .expect("resolve")
                .map(|symbol| symbol.symbol),
            None
        );
    }

    #[tokio::test]
    async fn local_symbol_directory_matches_simplified_query_to_traditional_name() {
        let pool = migrated_pool().await;
        seed_security_symbols(
            &pool,
            vec![
                security_symbol("0700.HK", "騰訊控股", "HK", "HKD", "stock"),
                security_symbol("0005.HK", "匯豐控股", "HK", "HKD", "stock"),
            ],
        )
        .await;

        assert_eq!(
            resolve_security_symbol(&pool, "腾讯控股", "HK", "HKD")
                .await
                .expect("resolve")
                .map(|symbol| symbol.symbol),
            Some("0700.HK".to_string())
        );
        assert_eq!(
            search_security_symbols(
                &pool,
                &SecuritySymbolSearchQuery {
                    q: Some("腾讯控股".to_string()),
                    market: Some("HK".to_string()),
                    currency: Some("HKD".to_string()),
                    limit: Some(10),
                },
            )
            .await
            .expect("search")
            .into_iter()
            .map(|symbol| symbol.symbol)
            .collect::<Vec<_>>(),
            vec!["0700.HK".to_string()]
        );
    }

    #[tokio::test]
    async fn hk_short_numeric_code_matches_and_commits_as_internal_symbol() {
        let pool = migrated_pool().await;
        seed_security_symbols(&pool, vec![security_symbol("0700.HK", "腾讯控股", "HK", "HKD", "stock")]).await;

        let search = search_security_symbols(
            &pool,
            &SecuritySymbolSearchQuery { q: Some("700".into()), market: Some("HK".into()), currency: Some("HKD".into()), limit: Some(10) },
        )
        .await
        .expect("search");
        assert_eq!(search.first().map(|symbol| symbol.symbol.as_str()), Some("0700.HK"));

        let row = PortfolioDraftRow {
            symbol: "700".into(), name: "腾讯控股".into(), quantity: "100".into(), average_cost: "300".into(),
            currency: "HKD".into(), account: None, market: "HK".into(), sector: None,
            imported_market_value: Some("32000".into()), last_price: None, notes: None, confidence: "high".into(),
            warnings: Vec::new(), errors: Vec::new(),
        };
        commit_draft_rows(
            &pool,
            Arc::new(MockMarketDataProvider),
            PortfolioDraftCommitRequest { rows: vec![row] },
        )
        .await
        .expect("commit");
        assert_eq!(list_positions(&pool).await.expect("positions")[0].symbol, "0700.HK");
    }

    #[tokio::test]
    async fn draft_commit_resolves_missing_symbol_from_local_directory() {
        let pool = migrated_pool().await;
        seed_security_symbols(
            &pool,
            vec![security_symbol("0700.HK", "腾讯控股", "HK", "HKD", "stock")],
        )
        .await;

        commit_draft_rows(
            &pool,
            Arc::new(MockMarketDataProvider),
            PortfolioDraftCommitRequest {
                rows: vec![PortfolioDraftRow {
                    symbol: "".to_string(),
                    name: "腾讯控股".to_string(),
                    quantity: "100".to_string(),
                    average_cost: "300".to_string(),
                    currency: "HKD".to_string(),
                    account: None,
                    market: "HK".to_string(),
                    sector: None,
                    imported_market_value: Some("32000".to_string()),
                    last_price: None,
                    notes: None,
                    confidence: "high".to_string(),
                    warnings: Vec::new(),
                    errors: Vec::new(),
                }],
            },
        )
        .await
        .expect("commit");

        let positions = list_positions(&pool).await.expect("positions");
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].symbol, "0700.HK");
    }

    #[tokio::test]
    async fn draft_symbol_resolution_updates_current_rows() {
        let pool = migrated_pool().await;
        seed_security_symbols(
            &pool,
            vec![security_symbol("159201.SZ", "自由现金", "CN", "CNY", "fund")],
        )
        .await;

        let result = resolve_draft_symbols_from_directory(
            &pool,
            PortfolioDraftSymbolResolveRequest {
                rows: vec![PortfolioDraftRow {
                    symbol: "".to_string(),
                    name: "自由现金".to_string(),
                    quantity: "13000".to_string(),
                    average_cost: "1.257".to_string(),
                    currency: "CNY".to_string(),
                    account: None,
                    market: "CN".to_string(),
                    sector: None,
                    imported_market_value: Some("14001.00".to_string()),
                    last_price: None,
                    notes: None,
                    confidence: "high".to_string(),
                    warnings: Vec::new(),
                    errors: vec!["symbol could not be resolved".to_string()],
                }],
            },
        )
        .await
        .expect("resolve");

        assert_eq!(result.resolved_count, 1);
        assert_eq!(result.draft_rows[0].symbol, "159201.SZ");
        assert!(result.draft_rows[0].errors.is_empty());
    }

    #[tokio::test]
    async fn draft_symbol_resolution_reuses_unique_existing_position() {
        let pool = migrated_pool().await;

        commit_draft_rows_with_symbol_resolver(
            &pool,
            Arc::new(MockMarketDataProvider),
            PortfolioDraftCommitRequest {
                rows: vec![PortfolioDraftRow {
                    symbol: "159201.SZ".to_string(),
                    name: "自由现金".to_string(),
                    quantity: "100".to_string(),
                    average_cost: "1.257".to_string(),
                    currency: "CNY".to_string(),
                    account: None,
                    market: "CN".to_string(),
                    sector: None,
                    imported_market_value: None,
                    last_price: Some("1.077".to_string()),
                    notes: None,
                    confidence: "high".to_string(),
                    warnings: Vec::new(),
                    errors: Vec::new(),
                }],
            },
            &FixedSymbolResolver { symbol: None },
        )
        .await
        .expect("seed position");

        let result = resolve_draft_symbols_from_directory(
            &pool,
            PortfolioDraftSymbolResolveRequest {
                rows: vec![PortfolioDraftRow {
                    symbol: "".to_string(),
                    name: "自由现金".to_string(),
                    quantity: "13000".to_string(),
                    average_cost: "1.257".to_string(),
                    currency: "CNY".to_string(),
                    account: None,
                    market: "CN".to_string(),
                    sector: None,
                    imported_market_value: Some("14001.00".to_string()),
                    last_price: Some("1.077".to_string()),
                    notes: None,
                    confidence: "high".to_string(),
                    warnings: Vec::new(),
                    errors: vec!["symbol is required".to_string()],
                }],
            },
        )
        .await
        .expect("resolve");

        assert_eq!(result.resolved_count, 1);
        assert_eq!(result.draft_rows[0].symbol, "159201.SZ");
        assert!(result.draft_rows[0].errors.is_empty());
    }

    include!("tests_public_symbols.rs");

    async fn migrated_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect sqlite");
        database::migrate(&pool).await.expect("migrate");
        pool
    }

    async fn seed_security_symbols(pool: &SqlitePool, symbols: Vec<SecuritySymbol>) {
        upsert_security_symbols(pool, &symbols)
            .await
            .expect("seed security symbols");
    }

    fn security_symbol(
        symbol: &str,
        name: &str,
        market: &str,
        currency: &str,
        asset_type: &str,
    ) -> SecuritySymbol {
        SecuritySymbol {
            symbol: symbol.to_string(),
            name: name.to_string(),
            market: market.to_string(),
            currency: currency.to_string(),
            asset_type: asset_type.to_string(),
            exchange: None,
            provider: "test".to_string(),
            updated_at: now_iso(),
        }
    }
}
