use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{ai::ConversationResearchSource, error::AppResult, time::now_iso};

use super::super::types::PersistedResearchSource;

pub(in crate::conversation) async fn insert_source(
    pool: &SqlitePool,
    run_id: &str,
    source: &ConversationResearchSource,
) -> AppResult<(PersistedResearchSource, bool)> {
    let existing = sqlx::query_as::<_, (String, String, String, String, String, String)>(
        r#"SELECT id, title, url, snippet, source_tier, retrieved_at
           FROM conversation_sources
           WHERE run_id = ? AND url = ?
           ORDER BY retrieved_at DESC
           LIMIT 1"#,
    )
    .bind(run_id)
    .bind(&source.url)
    .fetch_optional(pool)
    .await?;
    if let Some((id, title, url, snippet, source_tier, retrieved_at)) = existing {
        return Ok((
            PersistedResearchSource {
                id,
                title,
                url,
                snippet,
                source_tier,
                retrieved_at,
            },
            false,
        ));
    }

    let id = Uuid::new_v4().to_string();
    let retrieved_at = now_iso();
    sqlx::query(
        r#"INSERT INTO conversation_sources (
            id, run_id, title, url, snippet, source_tier, retrieved_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&id)
    .bind(run_id)
    .bind(&source.title)
    .bind(&source.url)
    .bind(&source.snippet)
    .bind(&source.source_tier)
    .bind(&retrieved_at)
    .execute(pool)
    .await?;
    Ok((
        PersistedResearchSource {
            id,
            title: source.title.clone(),
            url: source.url.clone(),
            snippet: source.snippet.clone(),
            source_tier: source.source_tier.clone(),
            retrieved_at,
        },
        true,
    ))
}
