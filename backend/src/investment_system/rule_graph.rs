use std::collections::{HashMap, HashSet, VecDeque};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    time::now_iso,
};

const DEFAULT_GRAPH_ID: &str = "default";

#[async_trait]
pub trait RuleNodeAdapter: Send + Sync {
    fn key(&self) -> &str;
    async fn execute(&self, input: Value, config: &Value) -> Result<Value, String>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleGraph {
    pub graph_id: String,
    pub name: String,
    #[serde(default)]
    pub nodes: Vec<RuleNode>,
    #[serde(default)]
    pub edges: Vec<RuleEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleNode {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub operation: String,
    #[serde(default)]
    pub config: Value,
    #[serde(default)]
    pub input_schema: Value,
    #[serde(default)]
    pub output_schema: Value,
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleEdge {
    pub id: String,
    pub from_node: String,
    pub to_node: String,
    #[serde(default)]
    pub condition: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleGraphPatch {
    pub base_version: i64,
    pub graph: RuleGraph,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleGraphVersion {
    pub id: String,
    pub graph_id: String,
    pub version: i64,
    pub status: String,
    pub graph: RuleGraph,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleNodeTrace {
    pub node_id: String,
    pub input: Value,
    pub output: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleEvaluation {
    pub execution_id: String,
    pub graph_version: i64,
    pub output: Value,
    pub trace: Vec<RuleNodeTrace>,
}

pub async fn active_rule_graph(pool: &SqlitePool) -> AppResult<RuleGraphVersion> {
    if let Some(version) = load_active(pool).await? {
        return Ok(version);
    }

    let graph = default_graph();
    insert_graph_version(pool, &graph, 1, None).await
}

pub async fn activate_rule_graph(
    pool: &SqlitePool,
    patch: RuleGraphPatch,
    action_id: Option<&str>,
) -> AppResult<RuleGraphVersion> {
    let active = active_rule_graph(pool).await?;
    if patch.base_version != active.version {
        return Err(AppError::bad_request(format!(
            "rule graph changed from version {} to {}; regenerate the proposal",
            patch.base_version, active.version
        )));
    }
    validate_graph(&patch.graph, &HashSet::new())?;

    let mut transaction = pool.begin().await?;
    sqlx::query(
        "UPDATE investment_rule_graph_versions SET status = 'superseded' WHERE graph_id = ? AND status = 'active'",
    )
    .bind(&patch.graph.graph_id)
    .execute(&mut *transaction)
    .await?;
    let version = active.version + 1;
    let id = Uuid::new_v4().to_string();
    let created_at = now_iso();
    sqlx::query(
        r#"INSERT INTO investment_rule_graph_versions (
            id, graph_id, version, status, graph_json, action_id, created_at
        ) VALUES (?, ?, ?, 'active', ?, ?, ?)"#,
    )
    .bind(&id)
    .bind(&patch.graph.graph_id)
    .bind(version)
    .bind(serde_json::to_string(&patch.graph)?)
    .bind(action_id)
    .bind(&created_at)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;

    Ok(RuleGraphVersion {
        id,
        graph_id: patch.graph.graph_id.clone(),
        version,
        status: "active".to_string(),
        graph: patch.graph,
        created_at,
    })
}

pub async fn evaluate_active_rule_graph(
    pool: &SqlitePool,
    input: Value,
) -> AppResult<RuleEvaluation> {
    let version = active_rule_graph(pool).await?;
    validate_graph(&version.graph, &HashSet::new())?;
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
        validate_json_shape(&node_input, &node.input_schema, "node input")?;
        let output = execute_fixed_node(node, &input, &incoming)?;
        validate_json_shape(&output, &node.output_schema, "node output")?;
        outputs.insert(node.id.clone(), output.clone());
        trace.push(RuleNodeTrace {
            node_id: node.id.clone(),
            input: node_input,
            output,
        });
    }

    let output = terminal_output(&version.graph, &outputs);
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

    Ok(RuleEvaluation {
        execution_id,
        graph_version: version.version,
        output,
        trace,
    })
}

pub async fn legacy_system(pool: &SqlitePool) -> AppResult<Option<Value>> {
    let value = sqlx::query_scalar::<_, String>(
        "SELECT content_json FROM investment_system_legacy WHERE id = 'default'",
    )
    .fetch_optional(pool)
    .await?;
    value
        .map(|value| serde_json::from_str(&value).map_err(AppError::from))
        .transpose()
}

pub fn validate_graph(graph: &RuleGraph, adapters: &HashSet<String>) -> AppResult<()> {
    if graph.graph_id.trim().is_empty() || graph.name.trim().is_empty() {
        return Err(AppError::bad_request("rule graph id and name are required"));
    }
    let mut ids = HashSet::new();
    for node in &graph.nodes {
        if node.id.trim().is_empty() || !ids.insert(node.id.clone()) {
            return Err(AppError::bad_request(
                "rule node ids must be unique and non-empty",
            ));
        }
        match node.kind.as_str() {
            "fixed" => validate_fixed_operation(&node.operation)?,
            "skill" | "agent" => {
                let adapter = node
                    .config
                    .get("adapter")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if adapter.is_empty() || !adapters.contains(adapter) {
                    return Err(AppError::bad_request(format!(
                        "node {} requires unavailable adapter {}",
                        node.id, adapter
                    )));
                }
            }
            _ => {
                return Err(AppError::bad_request(
                    "rule node kind must be fixed, skill, or agent",
                ))
            }
        }
    }
    for edge in &graph.edges {
        if !ids.contains(&edge.from_node) || !ids.contains(&edge.to_node) {
            return Err(AppError::bad_request(format!(
                "edge {} references a missing node",
                edge.id
            )));
        }
    }
    topological_order(graph)?;
    Ok(())
}

fn validate_fixed_operation(operation: &str) -> AppResult<()> {
    match operation {
        "input" | "compare" | "range" | "all" | "any" | "not" | "output" => Ok(()),
        _ => Err(AppError::bad_request(format!(
            "unsupported fixed operation {operation}"
        ))),
    }
}

fn topological_order(graph: &RuleGraph) -> AppResult<Vec<String>> {
    let mut indegree = graph
        .nodes
        .iter()
        .map(|node| (node.id.clone(), 0usize))
        .collect::<HashMap<_, _>>();
    let mut outgoing = HashMap::<String, Vec<String>>::new();
    for edge in &graph.edges {
        *indegree.entry(edge.to_node.clone()).or_default() += 1;
        outgoing
            .entry(edge.from_node.clone())
            .or_default()
            .push(edge.to_node.clone());
    }
    let mut queue = indegree
        .iter()
        .filter_map(|(id, degree)| (*degree == 0).then_some(id.clone()))
        .collect::<VecDeque<_>>();
    let mut order = Vec::new();
    while let Some(id) = queue.pop_front() {
        order.push(id.clone());
        for next in outgoing.get(&id).into_iter().flatten() {
            let degree = indegree.get_mut(next).expect("validated edge target");
            *degree -= 1;
            if *degree == 0 {
                queue.push_back(next.clone());
            }
        }
    }
    if order.len() != graph.nodes.len() {
        return Err(AppError::bad_request("rule graph must be acyclic"));
    }
    Ok(order)
}

fn execute_fixed_node(node: &RuleNode, context: &Value, incoming: &[Value]) -> AppResult<Value> {
    if node.kind != "fixed" {
        return Err(AppError::bad_request(format!(
            "node {} requires an adapter",
            node.id
        )));
    }
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

fn validate_json_shape(value: &Value, schema: &Value, label: &str) -> AppResult<()> {
    if schema.is_null() || schema.as_object().is_some_and(|object| object.is_empty()) {
        return Ok(());
    }
    if let Some(expected) = schema.get("type").and_then(Value::as_str) {
        let valid = match expected {
            "object" => value.is_object(),
            "array" => value.is_array(),
            "number" => value.is_number(),
            "string" => value.is_string(),
            "boolean" => value.is_boolean(),
            "null" => value.is_null(),
            _ => false,
        };
        if !valid {
            return Err(AppError::bad_request(format!(
                "{label} does not match schema type {expected}"
            )));
        }
    }
    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        let object = value
            .as_object()
            .ok_or_else(|| AppError::bad_request(format!("{label} must be an object")))?;
        for key in required.iter().filter_map(Value::as_str) {
            if !object.contains_key(key) {
                return Err(AppError::bad_request(format!(
                    "{label} is missing required field {key}"
                )));
            }
        }
    }
    Ok(())
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

async fn load_active(pool: &SqlitePool) -> AppResult<Option<RuleGraphVersion>> {
    let row = sqlx::query(
        r#"SELECT id, graph_id, version, status, graph_json, created_at
        FROM investment_rule_graph_versions
        WHERE graph_id = ? AND status = 'active'
        ORDER BY version DESC LIMIT 1"#,
    )
    .bind(DEFAULT_GRAPH_ID)
    .fetch_optional(pool)
    .await?;
    row.map(version_from_row).transpose()
}

async fn insert_graph_version(
    pool: &SqlitePool,
    graph: &RuleGraph,
    version: i64,
    action_id: Option<&str>,
) -> AppResult<RuleGraphVersion> {
    validate_graph(graph, &HashSet::new())?;
    let id = Uuid::new_v4().to_string();
    let created_at = now_iso();
    sqlx::query(
        r#"INSERT INTO investment_rule_graph_versions (
            id, graph_id, version, status, graph_json, action_id, created_at
        ) VALUES (?, ?, ?, 'active', ?, ?, ?)"#,
    )
    .bind(&id)
    .bind(&graph.graph_id)
    .bind(version)
    .bind(serde_json::to_string(graph)?)
    .bind(action_id)
    .bind(&created_at)
    .execute(pool)
    .await?;
    Ok(RuleGraphVersion {
        id,
        graph_id: graph.graph_id.clone(),
        version,
        status: "active".to_string(),
        graph: graph.clone(),
        created_at,
    })
}

fn version_from_row(row: sqlx::sqlite::SqliteRow) -> AppResult<RuleGraphVersion> {
    Ok(RuleGraphVersion {
        id: row.try_get("id")?,
        graph_id: row.try_get("graph_id")?,
        version: row.try_get("version")?,
        status: row.try_get("status")?,
        graph: serde_json::from_str(&row.try_get::<String, _>("graph_json")?)?,
        created_at: row.try_get("created_at")?,
    })
}

fn default_graph() -> RuleGraph {
    RuleGraph {
        graph_id: DEFAULT_GRAPH_ID.to_string(),
        name: "Default investment system".to_string(),
        nodes: vec![RuleNode {
            id: "result".to_string(),
            label: "Result".to_string(),
            kind: "fixed".to_string(),
            operation: "output".to_string(),
            config: Value::Null,
            input_schema: Value::Null,
            output_schema: Value::Null,
            x: 0.0,
            y: 0.0,
        }],
        edges: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_cycles() {
        let graph = RuleGraph {
            graph_id: "default".to_string(),
            name: "test".to_string(),
            nodes: vec![node("a", "all"), node("b", "all")],
            edges: vec![edge("a-b", "a", "b"), edge("b-a", "b", "a")],
        };
        assert!(validate_graph(&graph, &HashSet::new()).is_err());
    }

    fn node(id: &str, operation: &str) -> RuleNode {
        RuleNode {
            id: id.to_string(),
            label: id.to_string(),
            kind: "fixed".to_string(),
            operation: operation.to_string(),
            config: Value::Null,
            input_schema: Value::Null,
            output_schema: Value::Null,
            x: 0.0,
            y: 0.0,
        }
    }

    fn edge(id: &str, from: &str, to: &str) -> RuleEdge {
        RuleEdge {
            id: id.to_string(),
            from_node: from.to_string(),
            to_node: to.to_string(),
            condition: None,
        }
    }
}
