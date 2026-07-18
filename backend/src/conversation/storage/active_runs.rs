use std::collections::HashMap;

use serde_json::Value;
use sqlx::{Row, SqlitePool};

use crate::error::AppResult;

use super::{
    super::types::{ConversationActiveCapability, ConversationExecutionPlan, ConversationRun},
    rows::run_from_row,
    run_select,
};

pub async fn active_runs(pool: &SqlitePool) -> AppResult<Vec<ConversationRun>> {
    let rows = sqlx::query(&run_select(
        "WHERE status IN ('queued', 'running') ORDER BY started_at ASC",
    ))
    .fetch_all(pool)
    .await?;
    let mut runs = rows
        .into_iter()
        .map(run_from_row)
        .collect::<AppResult<Vec<_>>>()?;
    for run in &mut runs {
        run.active_capabilities = active_capabilities_for_run(pool, &run.id).await?;
        run.execution_plan = execution_plan_for_run(pool, &run.id).await?;
    }
    Ok(runs)
}

pub(super) async fn execution_plan_for_run(
    pool: &SqlitePool,
    run_id: &str,
) -> AppResult<Option<ConversationExecutionPlan>> {
    let rows = sqlx::query(
        r#"SELECT event_type, payload_json FROM conversation_run_events
        WHERE run_id = ? AND event_type IN ('run.plan.created', 'run.plan.step')
        ORDER BY event_id ASC"#,
    )
    .bind(run_id)
    .fetch_all(pool)
    .await?;
    let mut plan = None::<ConversationExecutionPlan>;
    for row in rows {
        let event_type = row.try_get::<String, _>("event_type")?;
        let payload = serde_json::from_str::<Value>(&row.try_get::<String, _>("payload_json")?)?;
        if event_type == "run.plan.created" {
            match serde_json::from_value(payload) {
                Ok(created) => plan = Some(created),
                Err(error) => tracing::warn!(%run_id, %error, "invalid persisted run plan ignored"),
            }
        } else if let Some(plan) = &mut plan {
            let step_id = payload.get("step_id").and_then(Value::as_str);
            let status = payload.get("status").and_then(Value::as_str);
            if let (Some(step_id), Some(status)) = (step_id, status) {
                if let Some(step) = plan.steps.iter_mut().find(|step| step.id == step_id) {
                    step.status = status.to_string();
                }
            }
        }
    }
    Ok(plan)
}

pub(super) async fn active_capabilities_for_run(
    pool: &SqlitePool,
    run_id: &str,
) -> AppResult<Vec<ConversationActiveCapability>> {
    let rows = sqlx::query(
        r#"SELECT event_type, payload_json FROM conversation_run_events
        WHERE run_id = ? AND event_type IN (
            'tool.started', 'tool.progress', 'tool.completed', 'tool.failed'
        ) ORDER BY event_id ASC"#,
    )
    .bind(run_id)
    .fetch_all(pool)
    .await?;
    let mut active = HashMap::<String, ConversationActiveCapability>::new();
    for row in rows {
        let event_type = row.try_get::<String, _>("event_type")?;
        let payload = serde_json::from_str::<Value>(&row.try_get::<String, _>("payload_json")?)?;
        let Some(call_id) = payload
            .get("call_id")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            continue;
        };
        if matches!(event_type.as_str(), "tool.started" | "tool.progress") {
            match serde_json::from_value::<ConversationActiveCapability>(payload) {
                Ok(capability) => {
                    active.insert(call_id, capability);
                }
                Err(error) => tracing::warn!(
                    %run_id,
                    %call_id,
                    %error,
                    "invalid persisted active capability event ignored"
                ),
            }
        } else {
            active.remove(&call_id);
        }
    }
    let mut active = active.into_values().collect::<Vec<_>>();
    active.sort_by(|left, right| {
        left.step_index
            .cmp(&right.step_index)
            .then(left.call_id.cmp(&right.call_id))
    });
    Ok(active)
}
