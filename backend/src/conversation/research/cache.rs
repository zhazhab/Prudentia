use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};

use super::ResearchOutcome;
use crate::error::AppResult;

pub(super) const RESEARCH_CACHE_TTL: chrono::Duration = chrono::Duration::hours(24);

pub(super) async fn load_cache(
    pool: &SqlitePool,
    hash: &str,
) -> AppResult<Option<ResearchOutcome>> {
    let row = sqlx::query(
        "SELECT results_json, fetched_at FROM conversation_research_cache WHERE query_hash = ?",
    )
    .bind(hash)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let raw: String = row.try_get("results_json")?;
    let outcome = serde_json::from_str::<ResearchOutcome>(&raw)?;
    let fetched_at: String = row.try_get("fetched_at")?;
    let fresh = chrono::DateTime::parse_from_rfc3339(&fetched_at)
        .map(|fetched| {
            chrono::Utc::now() - fetched.with_timezone(&chrono::Utc) < RESEARCH_CACHE_TTL
        })
        .unwrap_or(false);
    if !fresh {
        return Ok(None);
    }
    Ok(Some(outcome))
}

pub(super) async fn prune_expired_cache(pool: &SqlitePool) -> AppResult<()> {
    let cutoff = (chrono::Utc::now() - RESEARCH_CACHE_TTL).to_rfc3339();
    sqlx::query(
        r#"DELETE FROM conversation_research_cache
        WHERE julianday(fetched_at) IS NULL OR julianday(fetched_at) < julianday(?)"#,
    )
    .bind(cutoff)
    .execute(pool)
    .await?;
    Ok(())
}

pub(super) fn query_hash(query: &str) -> String {
    Sha256::digest(query.trim().as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
