    #[test]
    fn tushare_codes_are_mapped_to_internal_symbol_format() {
        assert_eq!(internal_symbol_from_tushare_code("600036.SH"), "600036.SS");
        assert_eq!(internal_symbol_from_tushare_code("159201.SZ"), "159201.SZ");
        assert_eq!(internal_symbol_from_tushare_code("00700.HK"), "0700.HK");
        assert_eq!(internal_symbol_from_tushare_code("09988.HK"), "9988.HK");
    }

    #[test]
    fn public_symbol_directory_parses_exchange_sources() {
        let sse_symbols = sse_symbols_from_suggest_text(
            r#"_t.push({val:"600000",val2:"浦发银行",val3:"pfyx"});_t.push({val:"510300",val2:"300ETF",val3:"300etf"});"#,
            "fund",
        );
        assert_eq!(sse_symbols.len(), 2);
        assert_eq!(sse_symbols[0].symbol, "600000.SS");
        assert_eq!(sse_symbols[0].name, "浦发银行");
        assert_eq!(sse_symbols[0].market, "CN");
        assert_eq!(sse_symbols[0].currency, "CNY");

        let mut hkex_header = vec![String::new(); 17];
        hkex_header[0] = "Stock Code".to_string();
        hkex_header[1] = "Name of Securities".to_string();
        hkex_header[2] = "Category".to_string();
        hkex_header[16] = "Trading Currency".to_string();
        let mut hkex_row = vec![String::new(); 17];
        hkex_row[0] = "00700".to_string();
        hkex_row[1] = "TENCENT HOLDINGS LTD".to_string();
        hkex_row[2] = "Equity".to_string();
        hkex_row[16] = "HKD".to_string();
        let hkex_symbols = hkex_symbols_from_rows(&[hkex_header, hkex_row]);
        assert_eq!(hkex_symbols.len(), 1);
        assert_eq!(hkex_symbols[0].symbol, "0700.HK");
        assert_eq!(hkex_symbols[0].asset_type, "stock");

        let nasdaq_symbols = nasdaq_symbols_from_listed_text(
            "Symbol|Security Name|Market Category|Test Issue|Financial Status|Round Lot Size|ETF|NextShares\nAAPL|Apple Inc. - Common Stock|Q|N|N|100|N|N\nTEST|Test Issue Inc.|Q|Y|N|100|N|N\nFile Creation Time: 07042026|||||||\n",
        );
        assert_eq!(nasdaq_symbols.len(), 1);
        assert_eq!(nasdaq_symbols[0].symbol, "AAPL");
        assert_eq!(nasdaq_symbols[0].market, "US");
        assert_eq!(nasdaq_symbols[0].currency, "USD");

        let other_symbols = nasdaq_symbols_from_other_listed_text(
            "ACT Symbol|Security Name|Exchange|CQS Symbol|ETF|Round Lot Size|Test Issue|NASDAQ Symbol\nSPY|SPDR S&P 500 ETF Trust|P|SPY|Y|100|N|SPY\n",
        );
        assert_eq!(other_symbols.len(), 1);
        assert_eq!(other_symbols[0].symbol, "SPY");
        assert_eq!(other_symbols[0].asset_type, "fund");
        assert_eq!(other_symbols[0].exchange.as_deref(), Some("NYSE Arca"));
    }

    #[test]
    fn public_asset_type_normalizes_chinese_hkex_categories() {
        assert_eq!(public_asset_type("股份", "騰訊控股"), "stock");
        assert_eq!(public_asset_type("股本", "招商銀行"), "stock");
        assert_eq!(public_asset_type("交易所買賣基金", "恒生科技ETF"), "fund");
    }

    #[test]
    fn hkex_sheet_xml_parser_reads_target_columns() {
        let rows = hkex_rows_from_sheet_xml(
            r#"<x:worksheet><x:sheetData>
            <x:row r="3"><x:c r="A3" t="str"><x:v>Stock Code</x:v></x:c><x:c r="B3" t="str"><x:v>Name of Securities</x:v></x:c><x:c r="C3" t="str"><x:v>Category</x:v></x:c><x:c r="Q3" t="str"><x:v>Trading Currency</x:v></x:c></x:row>
            <x:row r="4"><x:c r="A4" t="str"><x:v>00003</x:v></x:c><x:c r="B4" t="str"><x:v>HK &amp; CHINA GAS</x:v></x:c><x:c r="C4" t="str"><x:v>Equity</x:v></x:c><x:c r="Q4" t="str"><x:v>HKD</x:v></x:c></x:row>
            </x:sheetData></x:worksheet>"#,
        );
        let symbols = hkex_symbols_from_rows(&rows);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].symbol, "0003.HK");
        assert_eq!(symbols[0].name, "HK & CHINA GAS");
        assert_eq!(symbols[0].currency, "HKD");
    }

    #[test]
    fn public_symbol_source_cache_reads_fresh_files_for_24_hours() {
        let directory = tempfile::tempdir().expect("tempdir");
        let cache_path = directory.path().join("sse_stock.js");
        std::fs::write(&cache_path, b"cached-symbol-source").expect("write cache");
        let config = PublicSymbolDirectoryConfig {
            cache_dir: directory.path().to_string_lossy().to_string(),
            inventory_file: directory
                .path()
                .join("symbols.json")
                .to_string_lossy()
                .to_string(),
            cache_ttl_secs: Some(24 * 60 * 60),
            sources: Vec::new(),
        };

        assert_eq!(
            public_symbol_cache_ttl(&config),
            Duration::from_secs(24 * 60 * 60)
        );
        assert_eq!(
            read_fresh_public_symbol_cache(&cache_path, public_symbol_cache_ttl(&config))
                .expect("read cache")
                .expect("fresh cache"),
            b"cached-symbol-source"
        );
    }

    #[test]
    fn public_symbol_directory_config_declares_sources_and_cache_policy() {
        let config = load_public_symbol_directory_config().expect("load config");

        assert!(config
            .cache_dir
            .ends_with("data/symbol-directory/public"));
        assert!(config
            .inventory_file
            .ends_with("data/symbol-directory/public/symbols.json"));
        assert_eq!(config.cache_ttl_secs, Some(24 * 60 * 60));
        assert!(config.sources.iter().any(|source| {
            source.id == "hkex_zh"
                && source.kind == "hkex"
                && source.cache_file == "public_hkex_zh.xlsx"
        }));
        assert!(config
            .sources
            .iter()
            .any(|source| source.id == "sse_fund" && source.asset_type.as_deref() == Some("fund")));
    }

    #[test]
    fn public_symbol_inventory_round_trips_normalized_symbols_and_metadata() {
        let directory = tempfile::tempdir().expect("tempdir");
        let inventory_path = directory.path().join("symbols.json");
        let inventory = PublicSymbolInventory {
            schema_version: 1,
            updated_at: now_iso(),
            sources: vec![PublicSymbolInventorySource {
                id: "hkex_zh".to_string(),
                count: 1,
            }],
            symbols: vec![PublicSymbolInventorySymbol {
                symbol: "0700.HK".to_string(),
                name: "騰訊控股".to_string(),
                market: "HK".to_string(),
                currency: "HKD".to_string(),
            }],
        };

        write_public_symbol_inventory(&inventory_path, &inventory).expect("write inventory");
        let loaded = read_public_symbol_inventory(&inventory_path).expect("read inventory");

        assert_eq!(loaded.schema_version, 1);
        assert_eq!(loaded.sources[0].id, "hkex_zh");
        assert_eq!(loaded.sources[0].count, 1);
        assert_eq!(loaded.symbols[0].symbol, "0700.HK");
        let security_symbols = public_symbol_inventory_security_symbols(&loaded);
        assert_eq!(security_symbols[0].updated_at, loaded.updated_at);
        assert_eq!(security_symbols[0].asset_type, "security");
        assert_eq!(security_symbols[0].provider, "public:inventory");
        assert!(security_symbols[0].exchange.is_none());
        assert!(!public_symbol_inventory_is_expired(
            &loaded,
            Duration::from_secs(24 * 60 * 60)
        ));
    }

    #[test]
    fn public_symbol_inventory_serializes_only_file_level_updated_at() {
        let directory = tempfile::tempdir().expect("tempdir");
        let inventory_path = directory.path().join("symbols.json");
        let inventory = PublicSymbolInventory {
            schema_version: 1,
            updated_at: "2026-07-04T00:00:00Z".to_string(),
            sources: vec![PublicSymbolInventorySource {
                id: "hkex_zh".to_string(),
                count: 1,
            }],
            symbols: vec![PublicSymbolInventorySymbol {
                symbol: "0700.HK".to_string(),
                name: "騰訊控股".to_string(),
                market: "HK".to_string(),
                currency: "HKD".to_string(),
            }],
        };

        write_public_symbol_inventory(&inventory_path, &inventory).expect("write inventory");
        let json = serde_json::from_str::<serde_json::Value>(
            &std::fs::read_to_string(&inventory_path).expect("read inventory"),
        )
        .expect("parse inventory");

        assert_eq!(json["updated_at"], "2026-07-04T00:00:00Z");
        assert!(json["sources"][0].get("updated_at").is_none());
        assert!(json["sources"][0].get("provider").is_none());
        assert!(json["symbols"][0].get("updated_at").is_none());
        assert_eq!(
            json["symbols"][0]
                .as_object()
                .expect("symbol object")
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            vec!["currency", "market", "name", "symbol"]
        );
    }

    #[test]
    fn public_symbol_inventory_staleness_uses_inventory_updated_at() {
        let inventory = PublicSymbolInventory {
            schema_version: 1,
            updated_at: "2000-01-01T00:00:00Z".to_string(),
            sources: Vec::new(),
            symbols: Vec::new(),
        };

        assert!(public_symbol_inventory_is_expired(
            &inventory,
            Duration::from_secs(24 * 60 * 60)
        ));
    }

    #[test]
    fn public_symbol_inventory_normalization_lets_later_sources_override_duplicates() {
        let symbols = normalize_public_symbol_inventory_symbols(vec![
            SecuritySymbol {
                symbol: "0700.HK".to_string(),
                name: "TENCENT HOLDINGS LTD".to_string(),
                market: "HK".to_string(),
                currency: "HKD".to_string(),
                asset_type: "stock".to_string(),
                exchange: Some("HKEX".to_string()),
                provider: "public:hkex".to_string(),
                updated_at: now_iso(),
            },
            SecuritySymbol {
                symbol: "0700.HK".to_string(),
                name: "騰訊控股".to_string(),
                market: "HK".to_string(),
                currency: "HKD".to_string(),
                asset_type: "stock".to_string(),
                exchange: Some("HKEX".to_string()),
                provider: "public:hkex_zh".to_string(),
                updated_at: now_iso(),
            },
        ]);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "腾讯控股");
        assert_eq!(symbols[0].symbol, "0700.HK");
    }

    #[test]
    fn public_symbol_inventory_normalization_converts_traditional_names_to_simplified() {
        let symbols = normalize_public_symbol_inventory_symbols(vec![SecuritySymbol {
            symbol: "0005.HK".to_string(),
            name: "匯豐控股".to_string(),
            market: "HK".to_string(),
            currency: "HKD".to_string(),
            asset_type: "stock".to_string(),
            exchange: Some("HKEX".to_string()),
            provider: "public:hkex_zh".to_string(),
            updated_at: now_iso(),
        }]);

        assert_eq!(symbols[0].name, "汇丰控股");
    }

    #[test]
    fn image_rows_are_normalized_without_broker_specific_names() {
        let draft_rows = draft_rows_from_image_recognition_rows(vec![
            PortfolioImageDraftRow {
                symbol: "".to_string(),
                name: "Hong Kong Holding".to_string(),
                quantity: "900".to_string(),
                average_cost: "HK$489.877".to_string(),
                currency: "HK$".to_string(),
                account: Some("Broker Account".to_string()),
                market: Some("HK".to_string()),
                sector: None,
                imported_market_value: Some("335,646.34".to_string()),
                last_price: None,
                notes: Some("last price HK$430.200".to_string()),
                confidence: "high".to_string(),
                warnings: vec!["Symbol is not visible in the screenshot.".to_string()],
            },
            PortfolioImageDraftRow {
                symbol: "".to_string(),
                name: "Mainland Holding".to_string(),
                quantity: "2700".to_string(),
                average_cost: "37.894".to_string(),
                currency: "CNY".to_string(),
                account: Some("Broker Account".to_string()),
                market: None,
                sector: None,
                imported_market_value: Some("98,820.00".to_string()),
                last_price: None,
                notes: Some("last price 36.600".to_string()),
                confidence: "high".to_string(),
                warnings: vec!["Symbol and currency are not visible in the screenshot.".to_string()],
            },
            PortfolioImageDraftRow {
                symbol: "".to_string(),
                name: "Cash Flow ETF".to_string(),
                quantity: "13000".to_string(),
                average_cost: "1.257".to_string(),
                currency: "CNY".to_string(),
                account: Some("Broker Account".to_string()),
                market: None,
                sector: None,
                imported_market_value: Some("14,001.00".to_string()),
                last_price: None,
                notes: Some("last price 1.077".to_string()),
                confidence: "high".to_string(),
                warnings: Vec::new(),
            },
            PortfolioImageDraftRow {
                symbol: "".to_string(),
                name: "Available Cash".to_string(),
                quantity: "".to_string(),
                average_cost: "".to_string(),
                currency: "CNY".to_string(),
                account: Some("Broker Account".to_string()),
                market: None,
                sector: None,
                imported_market_value: Some("0.84".to_string()),
                last_price: None,
                notes: None,
                confidence: "high".to_string(),
                warnings: Vec::new(),
            },
        ]);

        assert_eq!(draft_rows.len(), 3);
        assert_eq!(draft_rows[0].name, "Hong Kong Holding");
        assert_eq!(draft_rows[0].average_cost, "489.877");
        assert_eq!(draft_rows[0].currency, "HKD");
        assert_eq!(draft_rows[0].market, "HK");
        assert_eq!(
            draft_rows[0].imported_market_value.as_deref(),
            Some("335646.34")
        );
        assert_eq!(draft_rows[0].last_price.as_deref(), Some("430.200"));
        assert!(draft_rows[0].errors.is_empty());

        assert_eq!(draft_rows[1].name, "Mainland Holding");
        assert_eq!(draft_rows[1].currency, "CNY");
        assert_eq!(draft_rows[1].market, "CN");
        assert_eq!(
            draft_rows[1].imported_market_value.as_deref(),
            Some("98820.00")
        );
        assert_eq!(draft_rows[1].last_price.as_deref(), Some("36.600"));
        assert!(draft_rows[1].errors.is_empty());

        assert_eq!(draft_rows[2].name, "Cash Flow ETF");
        assert_eq!(draft_rows[2].quantity, "13000");
        assert_eq!(draft_rows[2].average_cost, "1.257");
        assert_eq!(draft_rows[2].currency, "CNY");
        assert_eq!(draft_rows[2].market, "CN");
        assert_eq!(
            draft_rows[2].imported_market_value.as_deref(),
            Some("14001.00")
        );
        assert_eq!(draft_rows[2].last_price.as_deref(), Some("1.077"));
        assert!(draft_rows[2].errors.is_empty());
    }
