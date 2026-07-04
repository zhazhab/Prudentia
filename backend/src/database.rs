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
