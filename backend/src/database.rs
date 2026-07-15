use std::{fs, path::Path};

use sqlx::{Executor, Row, SqlitePool};

pub fn ensure_sqlite_file(database_url: &str) -> anyhow::Result<()> {
    let Some(path) = database_url.strip_prefix("sqlite://") else {
        return Ok(());
    };

    if path == ":memory:" {
        return Ok(());
    }

    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent)?;
    }

    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;

    Ok(())
}

pub async fn migrate(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    pool.execute("PRAGMA foreign_keys = ON;").await?;

    pool.execute(
        r#"
        CREATE TABLE IF NOT EXISTS memos (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            symbol TEXT,
            asset_type TEXT NOT NULL,
            thesis TEXT NOT NULL,
            risks TEXT NOT NULL,
            catalysts TEXT NOT NULL,
            disconfirming_evidence TEXT NOT NULL,
            notes TEXT NOT NULL,
            status TEXT NOT NULL,
            tags_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        "#,
    )
    .await?;

    pool.execute(
        r#"
        CREATE TABLE IF NOT EXISTS memo_threads (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            summary TEXT NOT NULL,
            status TEXT NOT NULL,
            linked_symbols_json TEXT NOT NULL,
            tags_json TEXT NOT NULL,
            archived_at TEXT,
            deleted_at TEXT,
            client_thread_id TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            last_message_at TEXT NOT NULL
        );
        "#,
    )
    .await?;

    pool.execute(
        r#"
        CREATE TABLE IF NOT EXISTS memo_thread_messages (
            id TEXT PRIMARY KEY,
            thread_id TEXT NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            status TEXT NOT NULL,
            request_id TEXT,
            duration_ms INTEGER,
            artifacts_json TEXT NOT NULL,
            sources_json TEXT NOT NULL,
            used_context_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(thread_id) REFERENCES memo_threads(id) ON DELETE CASCADE
        );
        "#,
    )
    .await?;

    pool.execute(
        r#"
        CREATE INDEX IF NOT EXISTS idx_memo_threads_active_recent
        ON memo_threads(deleted_at, archived_at, last_message_at, updated_at);
        "#,
    )
    .await?;

    pool.execute(
        r#"
        CREATE INDEX IF NOT EXISTS idx_memo_thread_messages_thread_created
        ON memo_thread_messages(thread_id, created_at);
        "#,
    )
    .await?;

    migrate_conversation_schema(pool).await?;

    pool.execute(
        r#"
        CREATE TABLE IF NOT EXISTS investment_system (
            id TEXT PRIMARY KEY,
            principles_json TEXT NOT NULL,
            checklist_items_json TEXT NOT NULL,
            circle_of_competence_json TEXT NOT NULL,
            decision_rules_json TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        "#,
    )
    .await?;

    pool.execute(
        r#"
        CREATE TABLE IF NOT EXISTS security_symbols (
            symbol TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            market TEXT NOT NULL,
            currency TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        "#,
    )
    .await?;
    migrate_security_symbols_schema(pool).await?;

    pool.execute("CREATE INDEX IF NOT EXISTS idx_security_symbols_name ON security_symbols(name);")
        .await?;

    pool.execute(
        "CREATE INDEX IF NOT EXISTS idx_security_symbols_market_currency ON security_symbols(market, currency);",
    )
    .await?;

    pool.execute(
        r#"
        CREATE TABLE IF NOT EXISTS portfolio_positions (
            symbol TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            asset_type TEXT NOT NULL,
            quantity REAL NOT NULL,
            average_cost REAL NOT NULL,
            currency TEXT NOT NULL,
            account TEXT,
            market TEXT,
            sector TEXT,
            notes TEXT,
            last_price REAL,
            market_value REAL NOT NULL,
            unrealized_pnl REAL NOT NULL,
            weight REAL NOT NULL,
            price_updated_at TEXT,
            price_stale INTEGER NOT NULL,
            updated_at TEXT NOT NULL
        );
        "#,
    )
    .await?;

    pool.execute(
        r#"
        CREATE TABLE IF NOT EXISTS portfolio_fx_rates (
            from_currency TEXT NOT NULL,
            to_currency TEXT NOT NULL,
            rate REAL NOT NULL,
            source TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            stale INTEGER NOT NULL,
            PRIMARY KEY(from_currency, to_currency)
        );
        "#,
    )
    .await?;

    pool.execute(
        r#"
        CREATE TABLE IF NOT EXISTS portfolio_performance_snapshots (
            id TEXT PRIMARY KEY,
            captured_at TEXT NOT NULL,
            source TEXT NOT NULL,
            base_currency TEXT NOT NULL,
            total_market_value_base REAL NOT NULL,
            total_cost_base REAL NOT NULL,
            total_unrealized_pnl_base REAL NOT NULL
        );
        "#,
    )
    .await?;

    pool.execute(
        r#"
        CREATE INDEX IF NOT EXISTS idx_portfolio_performance_snapshots_captured_at
        ON portfolio_performance_snapshots(captured_at);
        "#,
    )
    .await?;

    pool.execute(
        r#"
        CREATE TABLE IF NOT EXISTS portfolio_position_snapshots (
            id TEXT PRIMARY KEY,
            snapshot_id TEXT NOT NULL,
            symbol TEXT NOT NULL,
            name TEXT NOT NULL,
            currency TEXT NOT NULL,
            quantity REAL NOT NULL,
            average_cost REAL NOT NULL,
            market_value REAL NOT NULL,
            cost REAL NOT NULL,
            unrealized_pnl REAL NOT NULL,
            fx_rate REAL NOT NULL,
            value_base REAL NOT NULL,
            cost_base REAL NOT NULL,
            unrealized_pnl_base REAL NOT NULL,
            weight REAL NOT NULL,
            source TEXT NOT NULL,
            captured_at TEXT NOT NULL,
            FOREIGN KEY(snapshot_id) REFERENCES portfolio_performance_snapshots(id) ON DELETE CASCADE
        );
        "#,
    )
    .await?;

    pool.execute(
        r#"
        CREATE INDEX IF NOT EXISTS idx_portfolio_position_snapshots_symbol_time
        ON portfolio_position_snapshots(symbol, captured_at);
        "#,
    )
    .await?;

    pool.execute(
        r#"
        CREATE INDEX IF NOT EXISTS idx_portfolio_position_snapshots_snapshot
        ON portfolio_position_snapshots(snapshot_id);
        "#,
    )
    .await?;

    pool.execute(
        r#"
        CREATE TABLE IF NOT EXISTS portfolio_cash_flows (
            id TEXT PRIMARY KEY,
            occurred_at TEXT NOT NULL,
            flow_type TEXT NOT NULL,
            currency TEXT NOT NULL,
            amount REAL NOT NULL,
            fx_rate REAL NOT NULL,
            amount_base REAL NOT NULL,
            note TEXT,
            source TEXT NOT NULL,
            created_at TEXT NOT NULL
        );
        "#,
    )
    .await?;

    migrate_portfolio_ledger_schema(pool).await?;

    pool.execute(
        r#"
        CREATE INDEX IF NOT EXISTS idx_portfolio_cash_flows_occurred_at
        ON portfolio_cash_flows(occurred_at);
        "#,
    )
    .await?;

    pool.execute(
        r#"
        CREATE TABLE IF NOT EXISTS portfolio_benchmark_snapshots (
            id TEXT PRIMARY KEY,
            snapshot_id TEXT NOT NULL,
            benchmark_key TEXT NOT NULL,
            label TEXT NOT NULL,
            symbol TEXT NOT NULL,
            currency TEXT NOT NULL,
            price REAL,
            fx_rate REAL,
            value_base REAL,
            source TEXT,
            stale INTEGER NOT NULL,
            error TEXT,
            captured_at TEXT NOT NULL,
            FOREIGN KEY(snapshot_id) REFERENCES portfolio_performance_snapshots(id) ON DELETE CASCADE
        );
        "#,
    )
    .await?;

    pool.execute(
        r#"
        CREATE INDEX IF NOT EXISTS idx_portfolio_benchmark_snapshots_key_time
        ON portfolio_benchmark_snapshots(benchmark_key, captured_at);
        "#,
    )
    .await?;

    pool.execute(
        r#"
        CREATE TABLE IF NOT EXISTS portfolio_refresh_state (
            key TEXT PRIMARY KEY,
            attempted_at TEXT,
            succeeded_at TEXT,
            status TEXT NOT NULL,
            error TEXT
        );
        "#,
    )
    .await?;

    pool.execute(
        r#"
        CREATE TABLE IF NOT EXISTS decisions (
            id TEXT PRIMARY KEY,
            memo_id TEXT,
            symbol TEXT,
            action TEXT NOT NULL,
            rationale TEXT NOT NULL,
            confidence REAL NOT NULL,
            expected_outcome TEXT NOT NULL,
            review_date TEXT,
            created_at TEXT NOT NULL,
            FOREIGN KEY(memo_id) REFERENCES memos(id) ON DELETE SET NULL
        );
        "#,
    )
    .await?;

    pool.execute(
        r#"
        CREATE TABLE IF NOT EXISTS decision_delta_legs (
            id TEXT PRIMARY KEY,
            decision_id TEXT NOT NULL,
            leg_kind TEXT NOT NULL,
            baseline_type TEXT,
            symbol TEXT,
            quantity REAL,
            notional REAL,
            price REAL,
            currency TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(decision_id) REFERENCES decisions(id) ON DELETE CASCADE
        );
        "#,
    )
    .await?;

    pool.execute(
        r#"
        CREATE TABLE IF NOT EXISTS decision_delta_snapshots (
            id TEXT PRIMARY KEY,
            decision_id TEXT NOT NULL,
            as_of_date TEXT NOT NULL,
            actual_value REAL NOT NULL,
            baseline_value REAL NOT NULL,
            delta_value REAL NOT NULL,
            delta_pct REAL,
            portfolio_impact_pct REAL,
            price_used REAL,
            price_source TEXT,
            price_updated_at TEXT,
            fx_rate_used REAL,
            fx_source TEXT,
            fx_updated_at TEXT,
            price_stale INTEGER NOT NULL,
            fx_stale INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(decision_id) REFERENCES decisions(id) ON DELETE CASCADE
        );
        "#,
    )
    .await?;

    pool.execute(
        r#"
        CREATE TABLE IF NOT EXISTS decision_delta_reviews (
            decision_id TEXT PRIMARY KEY,
            notes TEXT NOT NULL,
            thesis_evidence_json TEXT NOT NULL,
            disconfirming_evidence_json TEXT NOT NULL,
            lessons_json TEXT NOT NULL,
            candidate_principles_json TEXT NOT NULL,
            candidate_checklist_items_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(decision_id) REFERENCES decisions(id) ON DELETE CASCADE
        );
        "#,
    )
    .await?;

    pool.execute(
        "CREATE INDEX IF NOT EXISTS idx_decision_delta_legs_decision_kind ON decision_delta_legs(decision_id, leg_kind);",
    )
    .await?;

    pool.execute(
        "CREATE INDEX IF NOT EXISTS idx_decision_delta_snapshots_latest ON decision_delta_snapshots(decision_id, created_at DESC, id DESC);",
    )
    .await?;

    pool.execute("CREATE INDEX IF NOT EXISTS idx_decisions_symbol ON decisions(symbol);")
        .await?;
    pool.execute("CREATE INDEX IF NOT EXISTS idx_decisions_action ON decisions(action);")
        .await?;
    pool.execute("CREATE INDEX IF NOT EXISTS idx_decisions_created_at ON decisions(created_at);")
        .await?;

    pool.execute(
        r#"
        CREATE TABLE IF NOT EXISTS research_records (
            id TEXT PRIMARY KEY,
            kind TEXT NOT NULL,
            title TEXT NOT NULL,
            source_type TEXT,
            source_title TEXT,
            source_author TEXT,
            source_content TEXT,
            symbol TEXT,
            memo_id TEXT,
            summary TEXT NOT NULL,
            insights_json TEXT NOT NULL,
            risks_json TEXT NOT NULL,
            checklist_json TEXT NOT NULL,
            candidate_principles_json TEXT NOT NULL,
            candidate_checklist_items_json TEXT NOT NULL,
            raw_output_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(memo_id) REFERENCES memos(id) ON DELETE SET NULL
        );
        "#,
    )
    .await?;

    migrate_investment_rule_graph_schema(pool).await?;

    Ok(())
}

async fn migrate_conversation_schema(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let statements = [
        r#"CREATE TABLE IF NOT EXISTS conversation_thread_subjects (
            thread_id TEXT PRIMARY KEY,
            kind TEXT NOT NULL,
            subject_key TEXT,
            label TEXT,
            confidence REAL NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(thread_id) REFERENCES memo_threads(id) ON DELETE CASCADE
        );"#,
        r#"CREATE TABLE IF NOT EXISTS conversation_runs (
            id TEXT PRIMARY KEY,
            client_request_id TEXT NOT NULL UNIQUE,
            thread_id TEXT NOT NULL,
            user_message_id TEXT NOT NULL,
            assistant_message_id TEXT,
            retry_of_run_id TEXT,
            status TEXT NOT NULL,
            phase TEXT NOT NULL,
            provider TEXT,
            task_complexity TEXT,
            model TEXT,
            route_reason TEXT,
            activity TEXT,
            source_count INTEGER,
            error_code TEXT,
            error_message TEXT,
            started_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            finished_at TEXT,
            FOREIGN KEY(thread_id) REFERENCES memo_threads(id) ON DELETE CASCADE,
            FOREIGN KEY(user_message_id) REFERENCES memo_thread_messages(id) ON DELETE CASCADE,
            FOREIGN KEY(assistant_message_id) REFERENCES memo_thread_messages(id) ON DELETE SET NULL
        );"#,
        r#"CREATE TABLE IF NOT EXISTS conversation_run_events (
            event_id INTEGER PRIMARY KEY AUTOINCREMENT,
            run_id TEXT NOT NULL,
            thread_id TEXT NOT NULL,
            event_type TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(run_id) REFERENCES conversation_runs(id) ON DELETE CASCADE,
            FOREIGN KEY(thread_id) REFERENCES memo_threads(id) ON DELETE CASCADE
        );"#,
        r#"CREATE TABLE IF NOT EXISTS conversation_turn_summaries (
            id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL UNIQUE,
            thread_id TEXT NOT NULL,
            subject_kind TEXT,
            subject_key TEXT,
            summary TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(run_id) REFERENCES conversation_runs(id) ON DELETE CASCADE,
            FOREIGN KEY(thread_id) REFERENCES memo_threads(id) ON DELETE CASCADE
        );"#,
        r#"CREATE TABLE IF NOT EXISTS conversation_actions (
            id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL,
            thread_id TEXT NOT NULL,
            action_type TEXT NOT NULL,
            title TEXT NOT NULL,
            rationale TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            result_json TEXT,
            target_version INTEGER,
            status TEXT NOT NULL,
            error TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            executed_at TEXT,
            FOREIGN KEY(run_id) REFERENCES conversation_runs(id) ON DELETE CASCADE,
            FOREIGN KEY(thread_id) REFERENCES memo_threads(id) ON DELETE CASCADE
        );"#,
        r#"CREATE TABLE IF NOT EXISTS conversation_attachments (
            id TEXT PRIMARY KEY,
            content_hash TEXT NOT NULL,
            file_name TEXT NOT NULL,
            mime_type TEXT NOT NULL,
            relative_path TEXT,
            source_url TEXT,
            extracted_text TEXT,
            parse_status TEXT NOT NULL,
            parse_error TEXT,
            size_bytes INTEGER NOT NULL,
            created_at TEXT NOT NULL
        );"#,
        r#"CREATE TABLE IF NOT EXISTS conversation_run_attachments (
            run_id TEXT NOT NULL,
            attachment_id TEXT NOT NULL,
            PRIMARY KEY(run_id, attachment_id),
            FOREIGN KEY(run_id) REFERENCES conversation_runs(id) ON DELETE CASCADE,
            FOREIGN KEY(attachment_id) REFERENCES conversation_attachments(id) ON DELETE CASCADE
        );"#,
        r#"CREATE TABLE IF NOT EXISTS conversation_sources (
            id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL,
            title TEXT NOT NULL,
            url TEXT NOT NULL,
            snippet TEXT NOT NULL,
            source_tier TEXT NOT NULL,
            retrieved_at TEXT NOT NULL,
            FOREIGN KEY(run_id) REFERENCES conversation_runs(id) ON DELETE CASCADE
        );"#,
        r#"CREATE TABLE IF NOT EXISTS conversation_research_cache (
            query_hash TEXT PRIMARY KEY,
            query TEXT NOT NULL,
            results_json TEXT NOT NULL,
            fetched_at TEXT NOT NULL
        );"#,
        r#"CREATE TABLE IF NOT EXISTS company_views (
            symbol TEXT PRIMARY KEY,
            company_name TEXT NOT NULL,
            current_version INTEGER NOT NULL,
            current_content_json TEXT NOT NULL,
            markdown_path TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );"#,
        r#"CREATE TABLE IF NOT EXISTS company_view_versions (
            id TEXT PRIMARY KEY,
            symbol TEXT NOT NULL,
            version INTEGER NOT NULL,
            content_json TEXT NOT NULL,
            action_id TEXT,
            provenance_json TEXT NOT NULL,
            markdown_path TEXT NOT NULL,
            created_at TEXT NOT NULL,
            UNIQUE(symbol, version),
            FOREIGN KEY(symbol) REFERENCES company_views(symbol) ON DELETE CASCADE,
            FOREIGN KEY(action_id) REFERENCES conversation_actions(id) ON DELETE SET NULL
        );"#,
        "CREATE INDEX IF NOT EXISTS idx_conversation_runs_thread_status ON conversation_runs(thread_id, status, started_at DESC);",
        "CREATE INDEX IF NOT EXISTS idx_conversation_events_id ON conversation_run_events(event_id);",
        "CREATE INDEX IF NOT EXISTS idx_conversation_actions_thread ON conversation_actions(thread_id, created_at);",
        "CREATE INDEX IF NOT EXISTS idx_conversation_sources_run ON conversation_sources(run_id);",
    ];
    for statement in statements {
        pool.execute(statement).await?;
    }
    ensure_table_column(pool, "conversation_runs", "task_complexity", "TEXT").await?;
    ensure_table_column(pool, "conversation_runs", "model", "TEXT").await?;
    ensure_table_column(pool, "conversation_runs", "route_reason", "TEXT").await?;
    ensure_table_column(pool, "conversation_runs", "activity", "TEXT").await?;
    ensure_table_column(pool, "conversation_runs", "source_count", "INTEGER").await?;
    ensure_table_column(pool, "conversation_turn_summaries", "subject_kind", "TEXT").await?;
    ensure_table_column(pool, "conversation_turn_summaries", "subject_key", "TEXT").await?;
    pool.execute("UPDATE conversation_turn_summaries SET subject_kind = COALESCE(subject_kind, (SELECT kind FROM conversation_thread_subjects WHERE thread_id = conversation_turn_summaries.thread_id)), subject_key = COALESCE(subject_key, (SELECT subject_key FROM conversation_thread_subjects WHERE thread_id = conversation_turn_summaries.thread_id));").await?;
    pool.execute(
        r#"DELETE FROM conversation_run_events
        WHERE event_type = 'message.delta'
          AND run_id IN (
            SELECT id FROM conversation_runs
            WHERE status IN ('completed', 'failed', 'canceled', 'interrupted')
          );"#,
    )
    .await?;
    Ok(())
}

async fn ensure_table_column(
    pool: &SqlitePool,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), sqlx::Error> {
    let rows = sqlx::query(&format!("PRAGMA table_info({table});"))
        .fetch_all(pool)
        .await?;
    if rows
        .iter()
        .any(|row| row.get::<String, _>("name") == column)
    {
        return Ok(());
    }
    pool.execute(format!("ALTER TABLE {table} ADD COLUMN {column} {definition};").as_str())
        .await?;
    Ok(())
}

async fn migrate_portfolio_ledger_schema(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let statements = [
        r#"CREATE TABLE IF NOT EXISTS portfolio_position_baselines (
            id TEXT PRIMARY KEY,
            symbol TEXT NOT NULL,
            effective_at TEXT NOT NULL,
            name TEXT NOT NULL,
            asset_type TEXT NOT NULL,
            quantity REAL NOT NULL,
            average_cost REAL NOT NULL,
            currency TEXT NOT NULL,
            account TEXT,
            market TEXT,
            sector TEXT,
            notes TEXT,
            last_price REAL,
            source TEXT NOT NULL,
            created_at TEXT NOT NULL
        );"#,
        r#"CREATE TABLE IF NOT EXISTS portfolio_trade_events (
            id TEXT PRIMARY KEY,
            event_kind TEXT NOT NULL,
            symbol TEXT NOT NULL,
            side TEXT,
            quantity REAL,
            price REAL,
            fees REAL,
            currency TEXT NOT NULL,
            occurred_at TEXT NOT NULL,
            fx_rate REAL,
            fx_source TEXT,
            amount_base REAL,
            impacts_portfolio INTEGER NOT NULL,
            reverses_trade_id TEXT,
            correction_of_trade_id TEXT,
            action_id TEXT,
            account TEXT,
            notes TEXT,
            created_at TEXT NOT NULL,
            FOREIGN KEY(reverses_trade_id) REFERENCES portfolio_trade_events(id),
            FOREIGN KEY(correction_of_trade_id) REFERENCES portfolio_trade_events(id),
            FOREIGN KEY(action_id) REFERENCES conversation_actions(id) ON DELETE SET NULL
        );"#,
        r#"CREATE TABLE IF NOT EXISTS portfolio_trade_cash_flows (
            trade_event_id TEXT PRIMARY KEY,
            cash_flow_id TEXT NOT NULL UNIQUE,
            FOREIGN KEY(trade_event_id) REFERENCES portfolio_trade_events(id) ON DELETE CASCADE,
            FOREIGN KEY(cash_flow_id) REFERENCES portfolio_cash_flows(id) ON DELETE CASCADE
        );"#,
        r#"CREATE TABLE IF NOT EXISTS portfolio_historical_fx_rates (
            from_currency TEXT NOT NULL,
            to_currency TEXT NOT NULL,
            rate_date TEXT NOT NULL,
            rate REAL NOT NULL,
            source TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY(from_currency, to_currency, rate_date)
        );"#,
        "CREATE INDEX IF NOT EXISTS idx_portfolio_baselines_symbol_time ON portfolio_position_baselines(symbol, effective_at DESC);",
        "CREATE INDEX IF NOT EXISTS idx_portfolio_trade_events_symbol_time ON portfolio_trade_events(symbol, occurred_at);",
    ];
    for statement in statements {
        pool.execute(statement).await?;
    }

    pool.execute(
        r#"INSERT INTO portfolio_position_baselines (
            id, symbol, effective_at, name, asset_type, quantity, average_cost, currency,
            account, market, sector, notes, last_price, source, created_at
        )
        SELECT lower(hex(randomblob(16))), p.symbol, p.updated_at, p.name, p.asset_type,
               p.quantity, p.average_cost, p.currency, p.account, p.market, p.sector,
               p.notes, p.last_price, 'migration', datetime('now')
        FROM portfolio_positions p
        WHERE NOT EXISTS (
            SELECT 1 FROM portfolio_position_baselines b WHERE b.symbol = p.symbol
        );"#,
    )
    .await?;
    Ok(())
}

async fn migrate_investment_rule_graph_schema(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let statements = [
        r#"CREATE TABLE IF NOT EXISTS investment_system_legacy (
            id TEXT PRIMARY KEY,
            content_json TEXT NOT NULL,
            migrated_at TEXT NOT NULL
        );"#,
        r#"CREATE TABLE IF NOT EXISTS investment_rule_graph_versions (
            id TEXT PRIMARY KEY,
            graph_id TEXT NOT NULL,
            version INTEGER NOT NULL,
            status TEXT NOT NULL,
            graph_json TEXT NOT NULL,
            action_id TEXT,
            created_at TEXT NOT NULL,
            UNIQUE(graph_id, version),
            FOREIGN KEY(action_id) REFERENCES conversation_actions(id) ON DELETE SET NULL
        );"#,
        r#"CREATE TABLE IF NOT EXISTS investment_rule_executions (
            id TEXT PRIMARY KEY,
            graph_version_id TEXT NOT NULL,
            input_json TEXT NOT NULL,
            trace_json TEXT NOT NULL,
            output_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(graph_version_id) REFERENCES investment_rule_graph_versions(id)
        );"#,
        "CREATE INDEX IF NOT EXISTS idx_rule_graph_active ON investment_rule_graph_versions(graph_id, status, version DESC);",
    ];
    for statement in statements {
        pool.execute(statement).await?;
    }
    pool.execute(
        r#"INSERT OR IGNORE INTO investment_system_legacy (id, content_json, migrated_at)
        SELECT id, json_object(
            'principles', json(principles_json),
            'checklist_items', json(checklist_items_json),
            'circle_of_competence', json(circle_of_competence_json),
            'decision_rules', json(decision_rules_json)
        ), datetime('now')
        FROM investment_system;"#,
    )
    .await?;
    Ok(())
}

async fn migrate_security_symbols_schema(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let rows = sqlx::query("PRAGMA table_info(security_symbols);")
        .fetch_all(pool)
        .await?;
    let columns = rows
        .into_iter()
        .map(|row| row.get::<String, _>("name"))
        .collect::<Vec<_>>();
    let expected = ["symbol", "name", "market", "currency", "updated_at"];
    if columns == expected {
        return Ok(());
    }

    pool.execute("ALTER TABLE security_symbols RENAME TO security_symbols_legacy;")
        .await?;
    pool.execute(
        r#"
        CREATE TABLE security_symbols (
            symbol TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            market TEXT NOT NULL,
            currency TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        "#,
    )
    .await?;
    pool.execute(
        r#"
        INSERT INTO security_symbols (symbol, name, market, currency, updated_at)
        SELECT symbol, name, market, currency, updated_at
        FROM security_symbols_legacy;
        "#,
    )
    .await?;
    pool.execute("DROP TABLE security_symbols_legacy;").await?;
    Ok(())
}
