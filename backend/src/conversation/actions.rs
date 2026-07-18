use std::{path::Path, sync::Arc};

use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::{
    ai::ConversationActionDraft,
    error::{AppError, AppResult},
    investment_system::{
        activate_rule_graph_with_adapters, active_rule_graph, RuleGraphPatch,
        RuleNodeAdapterRegistry,
    },
    market_data::MarketDataProvider,
    portfolio::{prepare_trade_record, record_trade, TradeRecord},
};

use super::{
    company::{apply_company_view_patch, load_company_view},
    storage,
    types::{CompanyViewPatch, ConversationAction},
};

pub async fn prepare_action(
    pool: &SqlitePool,
    market_data: Arc<dyn MarketDataProvider>,
    rule_node_adapters: &RuleNodeAdapterRegistry,
    mut draft: ConversationActionDraft,
) -> AppResult<(ConversationActionDraft, Option<i64>)> {
    match draft.action_type.as_str() {
        "company_view_patch" => {
            let mut patch: CompanyViewPatch = serde_json::from_value(draft.payload.clone())
                .map_err(|error| {
                    AppError::bad_request(format!("invalid company view proposal: {error}"))
                })?;
            patch.symbol = patch.symbol.trim().to_ascii_uppercase();
            strip_conversation_valuation_change(&mut patch);
            let version = load_company_view(pool, &patch.symbol)
                .await?
                .map(|view| view.current_version)
                .unwrap_or(0);
            patch.base_version = version;
            draft.payload = serde_json::to_value(patch)?;
            Ok((draft, Some(version)))
        }
        "trade_record" => {
            let trade: TradeRecord =
                serde_json::from_value(draft.payload.clone()).map_err(|error| {
                    AppError::bad_request(format!("invalid trade proposal: {error}"))
                })?;
            match prepare_trade_record(pool, market_data, trade).await {
                Ok(trade) => draft.payload = serde_json::to_value(trade)?,
                Err(error) => {
                    if let Some(object) = draft.payload.as_object_mut() {
                        object.insert("fx_error".to_string(), Value::String(error.to_string()));
                    }
                }
            }
            Ok((draft, None))
        }
        "rule_graph_patch" => {
            let mut patch: RuleGraphPatch =
                serde_json::from_value(draft.payload.clone()).map_err(|error| {
                    AppError::bad_request(format!("invalid rule graph proposal: {error}"))
                })?;
            let version = active_rule_graph(pool).await?.version;
            patch.base_version = version;
            rule_node_adapters.validate_graph(&patch.graph)?;
            draft.payload = serde_json::to_value(patch)?;
            Ok((draft, Some(version)))
        }
        other => Err(AppError::bad_request(format!(
            "unsupported conversation action {other}"
        ))),
    }
}

fn strip_conversation_valuation_change(patch: &mut CompanyViewPatch) {
    patch.changes.valuation_expectations = None;
}

pub async fn execute_action(
    pool: &SqlitePool,
    workspace_dir: &Path,
    market_data: Arc<dyn MarketDataProvider>,
    rule_node_adapters: &RuleNodeAdapterRegistry,
    action_id: &str,
    expected_version: Option<i64>,
) -> AppResult<ConversationAction> {
    let action = storage::action_by_id(pool, action_id).await?;
    if action.status == "executed" {
        return Ok(action);
    }
    if !matches!(action.status.as_str(), "proposed" | "edited" | "failed") {
        return Err(AppError::bad_request("action is not awaiting confirmation"));
    }
    if let (Some(expected), Some(target)) = (expected_version, action.target_version) {
        if expected != target {
            return Err(AppError::bad_request(
                "action target version does not match",
            ));
        }
    }
    storage::complete_action(pool, action_id, "executing", None, None).await?;
    let result = execute_action_inner(
        pool,
        workspace_dir,
        market_data,
        rule_node_adapters,
        &action,
    )
    .await;
    match result {
        Ok(result) => {
            storage::complete_action(pool, action_id, "executed", Some(result), None).await
        }
        Err(error) => {
            let _ =
                storage::complete_action(pool, action_id, "failed", None, Some(&error.to_string()))
                    .await;
            Err(error)
        }
    }
}

pub async fn reject_action(pool: &SqlitePool, action_id: &str) -> AppResult<ConversationAction> {
    let action = storage::action_by_id(pool, action_id).await?;
    if action.status == "rejected" {
        return Ok(action);
    }
    if !matches!(action.status.as_str(), "proposed" | "edited" | "failed") {
        return Err(AppError::bad_request("executed actions cannot be rejected"));
    }
    storage::complete_action(pool, action_id, "rejected", None, None).await
}

pub fn validate_edited_payload(action_type: &str, payload: &Value) -> AppResult<()> {
    match action_type {
        "company_view_patch" => {
            serde_json::from_value::<CompanyViewPatch>(payload.clone()).map_err(|error| {
                AppError::bad_request(format!("invalid company view proposal: {error}"))
            })?;
        }
        "trade_record" => {
            serde_json::from_value::<TradeRecord>(payload.clone()).map_err(|error| {
                AppError::bad_request(format!("invalid trade proposal: {error}"))
            })?;
        }
        "rule_graph_patch" => {
            serde_json::from_value::<RuleGraphPatch>(payload.clone()).map_err(|error| {
                AppError::bad_request(format!("invalid rule graph proposal: {error}"))
            })?;
        }
        _ => return Err(AppError::bad_request("unsupported conversation action")),
    }
    Ok(())
}

async fn execute_action_inner(
    pool: &SqlitePool,
    workspace_dir: &Path,
    market_data: Arc<dyn MarketDataProvider>,
    rule_node_adapters: &RuleNodeAdapterRegistry,
    action: &ConversationAction,
) -> AppResult<Value> {
    match action.action_type.as_str() {
        "company_view_patch" => {
            let mut patch: CompanyViewPatch = serde_json::from_value(action.payload.clone())?;
            strip_conversation_valuation_change(&mut patch);
            let view = apply_company_view_patch(
                pool,
                workspace_dir,
                patch,
                Some(&action.id),
                json!({ "run_id": action.run_id, "thread_id": action.thread_id, "action_id": action.id }),
            )
            .await?;
            Ok(serde_json::to_value(view)?)
        }
        "trade_record" => {
            let mut payload = action.payload.clone();
            if let Some(object) = payload.as_object_mut() {
                object.remove("fx_error");
            }
            let trade: TradeRecord = serde_json::from_value(payload)?;
            let receipt = record_trade(pool, market_data, trade, Some(&action.id)).await?;
            Ok(serde_json::to_value(receipt)?)
        }
        "rule_graph_patch" => {
            let patch: RuleGraphPatch = serde_json::from_value(action.payload.clone())?;
            let version = activate_rule_graph_with_adapters(
                pool,
                patch,
                Some(&action.id),
                rule_node_adapters,
            )
            .await?;
            Ok(serde_json::to_value(version)?)
        }
        _ => Err(AppError::bad_request("unsupported conversation action")),
    }
}

#[cfg(test)]
mod tests {
    use super::strip_conversation_valuation_change;
    use crate::conversation::types::{CompanyViewChanges, CompanyViewPatch};

    #[test]
    fn automatic_company_patch_cannot_propose_valuation_content() {
        let mut patch = CompanyViewPatch {
            symbol: "PDD".to_string(),
            company_name: "PDD Holdings".to_string(),
            base_version: 0,
            changes: CompanyViewChanges {
                business_quality: Some("operating evidence".to_string()),
                valuation_expectations: Some("must not persist".to_string()),
                ..CompanyViewChanges::default()
            },
        };

        strip_conversation_valuation_change(&mut patch);

        assert_eq!(
            patch.changes.business_quality.as_deref(),
            Some("operating evidence")
        );
        assert_eq!(patch.changes.valuation_expectations, None);
    }
}
