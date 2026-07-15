use sqlx::SqlitePool;

use crate::{error::AppResult, time::now_iso};

pub async fn set_run_task_assessment(
    pool: &SqlitePool,
    run_id: &str,
    complexity: &str,
    reason: &str,
) -> AppResult<()> {
    sqlx::query(
        r#"UPDATE conversation_runs SET task_complexity = ?, route_reason = ?, updated_at = ?
        WHERE id = ? AND status IN ('queued', 'running')"#,
    )
    .bind(complexity)
    .bind(reason)
    .bind(now_iso())
    .bind(run_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_run_model_route(
    pool: &SqlitePool,
    run_id: &str,
    provider: &str,
    model: &str,
    complexity: &str,
    reason: &str,
) -> AppResult<()> {
    sqlx::query(
        r#"UPDATE conversation_runs SET provider = ?, model = ?, task_complexity = ?,
                  route_reason = ?, updated_at = ?
        WHERE id = ? AND status IN ('queued', 'running')"#,
    )
    .bind(provider)
    .bind(model)
    .bind(complexity)
    .bind(reason)
    .bind(now_iso())
    .bind(run_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn run_has_attachments(pool: &SqlitePool, run_id: &str) -> AppResult<bool> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM conversation_run_attachments WHERE run_id = ?",
    )
    .bind(run_id)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}
