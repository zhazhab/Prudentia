use std::{fs, path::Path};

use sqlx::{Executor, SqlitePool};

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
