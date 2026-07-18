use std::{collections::HashMap, time::Duration};

use serde_json::{json, Value};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    time::now_iso,
};

use super::{
    active_rule_graph, ensure_json_size, topological_order, RuleEvaluation, RuleGraph, RuleNode,
    RuleNodeAdapterRegistry, RuleNodeTrace, MAX_RETAINED_RULE_EXECUTIONS,
    MAX_RULE_EXECUTION_INPUT_BYTES, MAX_RULE_EXECUTION_SECONDS, MAX_RULE_NODE_OUTPUT_BYTES,
    MAX_RULE_TRACE_BYTES,
};

pub async fn evaluate_active_rule_graph(
    pool: &SqlitePool,
    input: Value,
) -> AppResult<RuleEvaluation> {
    evaluate_active_rule_graph_with_adapters(pool, input, &RuleNodeAdapterRegistry::default()).await
}

pub async fn evaluate_active_rule_graph_with_adapters(
    pool: &SqlitePool,
    input: Value,
    adapters: &RuleNodeAdapterRegistry,
) -> AppResult<RuleEvaluation> {
    tokio::time::timeout(
        Duration::from_secs(MAX_RULE_EXECUTION_SECONDS),
        evaluate_active_rule_graph_inner(pool, input, adapters),
    )
    .await
    .map_err(|_| AppError::bad_request("rule graph execution exceeded its total deadline"))?
}

async fn evaluate_active_rule_graph_inner(
    pool: &SqlitePool,
    input: Value,
    adapters: &RuleNodeAdapterRegistry,
) -> AppResult<RuleEvaluation> {
    ensure_json_size(&input, MAX_RULE_EXECUTION_INPUT_BYTES, "rule graph input")?;
    let version = active_rule_graph(pool).await?;
    adapters.validate_pinned_graph(&version.graph)?;
    let order = topological_order(&version.graph)?;
    let mut outputs = HashMap::<String, Value>::new();
    let mut trace = Vec::new();

    for node_id in order {
        let node = version
            .graph
            .nodes
            .iter()
            .find(|node| node.id == node_id)
            .expect("validated node exists");
        let incoming = incoming_values(&version.graph, &node.id, &outputs);
        let node_input = json!({ "context": input, "incoming": incoming });
        validate_rule_value(&node_input, &node.input_schema, "node input")?;
        let output = if node.kind == "fixed" {
            execute_fixed_node(node, &input, &incoming)?
        } else {
            let adapter = node
                .config
                .get("adapter")
                .and_then(Value::as_str)
                .expect("validated capability node adapter");
            adapters
                .execute(adapter, node_input.clone(), &node.config)
                .await?
        };
        validate_rule_value(&output, &node.output_schema, "node output")?;
        ensure_json_size(&output, MAX_RULE_NODE_OUTPUT_BYTES, "rule node output")?;
        outputs.insert(node.id.clone(), output.clone());
        trace.push(RuleNodeTrace {
            node_id: node.id.clone(),
            input: json!({
                "context_ref": "execution_input",
                "incoming": incoming
            }),
            output,
        });
    }

    let output = terminal_output(&version.graph, &outputs);
    ensure_json_size(&trace, MAX_RULE_TRACE_BYTES, "rule graph trace")?;
    let execution_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"INSERT INTO investment_rule_executions (
            id, graph_version_id, input_json, trace_json, output_json, created_at
        ) VALUES (?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&execution_id)
    .bind(&version.id)
    .bind(serde_json::to_string(&input)?)
    .bind(serde_json::to_string(&trace)?)
    .bind(serde_json::to_string(&output)?)
    .bind(now_iso())
    .execute(pool)
    .await?;
    prune_rule_executions(pool).await?;

    Ok(RuleEvaluation {
        execution_id,
        graph_version: version.version,
        output,
        trace,
    })
}

fn execute_fixed_node(node: &RuleNode, context: &Value, incoming: &[Value]) -> AppResult<Value> {
    match node.operation.as_str() {
        "input" => {
            let key = node
                .config
                .get("key")
                .and_then(Value::as_str)
                .unwrap_or_default();
            Ok(context.get(key).cloned().unwrap_or(Value::Null))
        }
        "compare" => compare_value(node, context, incoming),
        "range" => range_value(node, context, incoming),
        "all" => Ok(Value::Bool(incoming.iter().all(truthy))),
        "any" => Ok(Value::Bool(incoming.iter().any(truthy))),
        "not" => Ok(Value::Bool(!incoming.first().is_some_and(truthy))),
        "output" => Ok(incoming.first().cloned().unwrap_or_else(|| context.clone())),
        _ => Err(AppError::bad_request("unsupported fixed operation")),
    }
}

fn compare_value(node: &RuleNode, context: &Value, incoming: &[Value]) -> AppResult<Value> {
    let left = operand(node, context, incoming);
    let right = node.config.get("value").cloned().unwrap_or(Value::Null);
    let operator = node
        .config
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or("eq");
    let result = match operator {
        "eq" => left == right,
        "ne" => left != right,
        "gt" | "gte" | "lt" | "lte" => {
            let left = left
                .as_f64()
                .ok_or_else(|| AppError::bad_request("compare input must be numeric"))?;
            let right = right
                .as_f64()
                .ok_or_else(|| AppError::bad_request("compare value must be numeric"))?;
            match operator {
                "gt" => left > right,
                "gte" => left >= right,
                "lt" => left < right,
                _ => left <= right,
            }
        }
        _ => return Err(AppError::bad_request("compare operator is invalid")),
    };
    Ok(Value::Bool(result))
}

fn range_value(node: &RuleNode, context: &Value, incoming: &[Value]) -> AppResult<Value> {
    let value = operand(node, context, incoming)
        .as_f64()
        .ok_or_else(|| AppError::bad_request("range input must be numeric"))?;
    let min = node
        .config
        .get("min")
        .and_then(Value::as_f64)
        .unwrap_or(f64::NEG_INFINITY);
    let max = node
        .config
        .get("max")
        .and_then(Value::as_f64)
        .unwrap_or(f64::INFINITY);
    Ok(Value::Bool(value >= min && value <= max))
}

fn operand(node: &RuleNode, context: &Value, incoming: &[Value]) -> Value {
    incoming.first().cloned().unwrap_or_else(|| {
        node.config
            .get("key")
            .and_then(Value::as_str)
            .and_then(|key| context.get(key))
            .cloned()
            .unwrap_or(Value::Null)
    })
}

fn truthy(value: &Value) -> bool {
    value.as_bool().unwrap_or(false)
}

fn validate_rule_value(value: &Value, schema: &Value, label: &str) -> AppResult<()> {
    if schema.is_null() || schema.as_object().is_some_and(|object| object.is_empty()) {
        return Ok(());
    }
    crate::json_schema::validate_json_schema(value, schema, label).map_err(AppError::bad_request)
}

fn incoming_values(
    graph: &RuleGraph,
    node_id: &str,
    outputs: &HashMap<String, Value>,
) -> Vec<Value> {
    graph
        .edges
        .iter()
        .filter(|edge| edge.to_node == node_id)
        .filter(|edge| edge.condition.as_ref().is_none_or(truthy))
        .filter_map(|edge| outputs.get(&edge.from_node).cloned())
        .collect()
}

fn terminal_output(graph: &RuleGraph, outputs: &HashMap<String, Value>) -> Value {
    graph
        .nodes
        .iter()
        .rev()
        .find(|node| node.operation == "output")
        .and_then(|node| outputs.get(&node.id))
        .cloned()
        .unwrap_or(Value::Null)
}

async fn prune_rule_executions(pool: &SqlitePool) -> AppResult<()> {
    sqlx::query(
        r#"DELETE FROM investment_rule_executions
        WHERE id NOT IN (
            SELECT id FROM investment_rule_executions
            ORDER BY created_at DESC, id DESC
            LIMIT ?
        )"#,
    )
    .bind(MAX_RETAINED_RULE_EXECUTIONS)
    .execute(pool)
    .await?;
    Ok(())
}
