use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    time::now_iso,
};

const DEFAULT_GRAPH_ID: &str = "default";
const MAX_RULE_GRAPH_NODES: usize = 64;
const MAX_RULE_GRAPH_MODEL_NODES: usize = 8;
const MAX_RULE_GRAPH_EDGES: usize = 256;
const MAX_RULE_GRAPH_BYTES: usize = 512 * 1024;
const MAX_RULE_EXECUTION_INPUT_BYTES: usize = 64 * 1024;
const MAX_RULE_NODE_OUTPUT_BYTES: usize = 64 * 1024;
const MAX_RULE_TRACE_BYTES: usize = 1024 * 1024;
const MAX_RETAINED_RULE_EXECUTIONS: i64 = 500;
const MAX_RULE_EXECUTION_SECONDS: u64 = 600;

mod evaluation;
pub use evaluation::{evaluate_active_rule_graph, evaluate_active_rule_graph_with_adapters};

#[async_trait]
pub trait RuleNodeAdapter: Send + Sync {
    fn key(&self) -> &str;
    fn kind(&self) -> &str {
        "skill"
    }
    fn manifest_hash(&self) -> Option<&str> {
        None
    }
    fn validate_config(&self, _config: &Value) -> Result<(), String> {
        Ok(())
    }
    async fn execute(&self, input: Value, config: &Value) -> Result<Value, String>;
}

#[derive(Clone, Default)]
pub struct RuleNodeAdapterRegistry {
    adapters: Arc<HashMap<String, Arc<dyn RuleNodeAdapter>>>,
}

impl RuleNodeAdapterRegistry {
    pub fn from_adapters(
        adapters: impl IntoIterator<Item = Arc<dyn RuleNodeAdapter>>,
    ) -> AppResult<Self> {
        let mut registered = HashMap::new();
        for adapter in adapters {
            let key = adapter.key().trim().to_string();
            if key.is_empty() {
                return Err(AppError::bad_request(
                    "rule node adapter keys cannot be empty",
                ));
            }
            if registered.insert(key.clone(), adapter).is_some() {
                return Err(AppError::bad_request(format!(
                    "rule node adapter {key} is registered more than once"
                )));
            }
        }
        Ok(Self {
            adapters: Arc::new(registered),
        })
    }

    pub fn available_keys(&self) -> HashSet<String> {
        self.adapters.keys().cloned().collect()
    }

    pub fn validate_graph(&self, graph: &RuleGraph) -> AppResult<()> {
        validate_graph(graph, &self.available_keys())?;
        self.validate_graph_kinds(graph)
    }

    pub fn pin_graph(&self, graph: &mut RuleGraph) -> AppResult<()> {
        self.validate_graph(graph)?;
        for node in graph
            .nodes
            .iter_mut()
            .filter(|node| matches!(node.kind.as_str(), "skill" | "agent"))
        {
            let key = node
                .config
                .get("adapter")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let adapter = self.adapters.get(key).ok_or_else(|| {
                AppError::bad_request(format!("rule node adapter {key} is unavailable"))
            })?;
            if let Some(hash) = adapter.manifest_hash() {
                node.config
                    .as_object_mut()
                    .expect("validated adapter config is an object")
                    .insert("manifest_hash".to_string(), Value::String(hash.to_string()));
            }
        }
        self.validate_pinned_graph(graph)
    }

    fn validate_pinned_graph(&self, graph: &RuleGraph) -> AppResult<()> {
        self.validate_graph(graph)?;
        for node in graph
            .nodes
            .iter()
            .filter(|node| matches!(node.kind.as_str(), "skill" | "agent"))
        {
            let key = node
                .config
                .get("adapter")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let adapter = self.adapters.get(key).ok_or_else(|| {
                AppError::bad_request(format!("rule node adapter {key} is unavailable"))
            })?;
            if let Some(current_hash) = adapter.manifest_hash() {
                let pinned_hash = node
                    .config
                    .get("manifest_hash")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if pinned_hash != current_hash {
                    return Err(AppError::bad_request(format!(
                        "node {} pins a different manifest for adapter {}; activate a new graph version",
                        node.id, key
                    )));
                }
            }
        }
        Ok(())
    }

    async fn execute(&self, key: &str, input: Value, config: &Value) -> AppResult<Value> {
        let adapter = self.adapters.get(key).ok_or_else(|| {
            AppError::bad_request(format!("rule node adapter {key} is unavailable"))
        })?;
        adapter
            .execute(input, config)
            .await
            .map_err(|error| AppError::bad_request(format!("adapter {key} failed: {error}")))
    }

    fn validate_graph_kinds(&self, graph: &RuleGraph) -> AppResult<()> {
        for node in graph
            .nodes
            .iter()
            .filter(|node| matches!(node.kind.as_str(), "skill" | "agent"))
        {
            let key = node
                .config
                .get("adapter")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let adapter = self.adapters.get(key).ok_or_else(|| {
                AppError::bad_request(format!("rule node adapter {key} is unavailable"))
            })?;
            if adapter.kind() != node.kind {
                return Err(AppError::bad_request(format!(
                    "node {} is kind {} but adapter {} is kind {}",
                    node.id,
                    node.kind,
                    key,
                    adapter.kind()
                )));
            }
            let expected_operation = key.split_once('@').map_or(key, |(name, _)| name);
            if node.operation != expected_operation {
                return Err(AppError::bad_request(format!(
                    "node {} operation must match adapter capability {}",
                    node.id, expected_operation
                )));
            }
            adapter.validate_config(&node.config).map_err(|error| {
                AppError::bad_request(format!(
                    "node {} configuration is invalid: {error}",
                    node.id
                ))
            })?;
            if let (Some(current_hash), Some(pinned_hash)) = (
                adapter.manifest_hash(),
                node.config.get("manifest_hash").and_then(Value::as_str),
            ) {
                if current_hash != pinned_hash {
                    return Err(AppError::bad_request(format!(
                        "node {} manifest hash does not match adapter {}",
                        node.id, key
                    )));
                }
            }
        }
        Ok(())
    }
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
    activate_rule_graph_with_adapters(pool, patch, action_id, &RuleNodeAdapterRegistry::default())
        .await
}

pub async fn activate_rule_graph_with_adapters(
    pool: &SqlitePool,
    patch: RuleGraphPatch,
    action_id: Option<&str>,
    adapters: &RuleNodeAdapterRegistry,
) -> AppResult<RuleGraphVersion> {
    let RuleGraphPatch {
        base_version,
        mut graph,
    } = patch;
    let active = active_rule_graph(pool).await?;
    if base_version != active.version {
        return Err(AppError::bad_request(format!(
            "rule graph changed from version {} to {}; regenerate the proposal",
            base_version, active.version
        )));
    }
    adapters.pin_graph(&mut graph)?;

    let mut transaction = pool.begin().await?;
    sqlx::query(
        "UPDATE investment_rule_graph_versions SET status = 'superseded' WHERE graph_id = ? AND status = 'active'",
    )
    .bind(&graph.graph_id)
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
    .bind(&graph.graph_id)
    .bind(version)
    .bind(serde_json::to_string(&graph)?)
    .bind(action_id)
    .bind(&created_at)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;

    Ok(RuleGraphVersion {
        id,
        graph_id: graph.graph_id.clone(),
        version,
        status: "active".to_string(),
        graph,
        created_at,
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
    ensure_json_size(graph, MAX_RULE_GRAPH_BYTES, "rule graph")?;
    if graph.nodes.len() > MAX_RULE_GRAPH_NODES || graph.edges.len() > MAX_RULE_GRAPH_EDGES {
        return Err(AppError::bad_request(format!(
            "rule graph exceeds the limit of {MAX_RULE_GRAPH_NODES} nodes or {MAX_RULE_GRAPH_EDGES} edges"
        )));
    }
    let model_node_count = graph
        .nodes
        .iter()
        .filter(|node| matches!(node.kind.as_str(), "skill" | "agent"))
        .count();
    if model_node_count > MAX_RULE_GRAPH_MODEL_NODES {
        return Err(AppError::bad_request(format!(
            "rule graph exceeds the limit of {MAX_RULE_GRAPH_MODEL_NODES} model-backed nodes"
        )));
    }
    let mut ids = HashSet::new();
    for node in &graph.nodes {
        if node.id.trim().is_empty()
            || node.label.trim().is_empty()
            || node.operation.trim().is_empty()
            || !ids.insert(node.id.clone())
        {
            return Err(AppError::bad_request(
                "rule nodes require unique ids, labels, and operations",
            ));
        }
        validate_rule_schema(&node.input_schema, "rule node input_schema")?;
        validate_rule_schema(&node.output_schema, "rule node output_schema")?;
        match node.kind.as_str() {
            "fixed" => validate_fixed_operation(&node.operation)?,
            "skill" | "agent" => {
                if !node.config.is_object() {
                    return Err(AppError::bad_request(format!(
                        "node {} configuration must be an object",
                        node.id
                    )));
                }
                let adapter = node
                    .config
                    .get("adapter")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let version_is_exact = adapter.rsplit_once('@').is_some_and(|(name, version)| {
                    !name.is_empty() && version.parse::<u16>().is_ok()
                });
                if !version_is_exact || !adapters.contains(adapter) {
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
    let mut edge_ids = HashSet::new();
    for edge in &graph.edges {
        if edge.id.trim().is_empty()
            || !edge_ids.insert(edge.id.clone())
            || !ids.contains(&edge.from_node)
            || !ids.contains(&edge.to_node)
        {
            return Err(AppError::bad_request(format!(
                "edge {} must have a unique id and reference existing nodes",
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

fn validate_rule_schema(schema: &Value, label: &str) -> AppResult<()> {
    if schema.is_null() || schema.as_object().is_some_and(|object| object.is_empty()) {
        return Ok(());
    }
    crate::json_schema::validate_schema_contract(schema, label).map_err(AppError::bad_request)
}

fn ensure_json_size<T: Serialize>(value: &T, limit: usize, label: &str) -> AppResult<()> {
    let size = serde_json::to_vec(value)?.len();
    if size > limit {
        return Err(AppError::bad_request(format!(
            "{label} is {size} bytes; the limit is {limit} bytes"
        )));
    }
    Ok(())
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
    use serde_json::json;
    use sqlx::sqlite::SqlitePoolOptions;

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

    struct ThresholdAdapter;

    #[async_trait]
    impl RuleNodeAdapter for ThresholdAdapter {
        fn key(&self) -> &str {
            "threshold_check@1"
        }

        fn manifest_hash(&self) -> Option<&str> {
            Some("threshold-manifest-v1")
        }

        fn validate_config(&self, config: &Value) -> Result<(), String> {
            config
                .get("threshold")
                .and_then(Value::as_i64)
                .map(|_| ())
                .ok_or_else(|| "threshold must be an integer".to_string())
        }

        async fn execute(&self, input: Value, config: &Value) -> Result<Value, String> {
            let threshold = config
                .get("threshold")
                .and_then(Value::as_i64)
                .unwrap_or(70);
            Ok(Value::Bool(
                input
                    .get("context")
                    .and_then(|context| context.get("score"))
                    .and_then(Value::as_i64)
                    .is_some_and(|score| score >= threshold),
            ))
        }
    }

    #[tokio::test]
    async fn registered_adapter_executes_inside_the_versioned_graph() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect sqlite");
        crate::database::migrate(&pool).await.expect("migrate");
        let adapters = RuleNodeAdapterRegistry::from_adapters([
            Arc::new(ThresholdAdapter) as Arc<dyn RuleNodeAdapter>
        ])
        .expect("adapter registry");
        let graph = RuleGraph {
            graph_id: DEFAULT_GRAPH_ID.to_string(),
            name: "adapter graph".to_string(),
            nodes: vec![
                RuleNode {
                    id: "threshold".to_string(),
                    label: "Threshold".to_string(),
                    kind: "skill".to_string(),
                    operation: "threshold_check".to_string(),
                    config: json!({
                        "adapter": "threshold_check@1",
                        "threshold": 70
                    }),
                    input_schema: json!({ "type": "object", "required": ["context"] }),
                    output_schema: json!({ "type": "boolean" }),
                    x: 0.0,
                    y: 0.0,
                },
                node("result", "output"),
            ],
            edges: vec![edge("threshold-result", "threshold", "result")],
        };

        let activated = activate_rule_graph_with_adapters(
            &pool,
            RuleGraphPatch {
                base_version: 1,
                graph,
            },
            None,
            &adapters,
        )
        .await
        .expect("activate graph");
        let evaluated =
            evaluate_active_rule_graph_with_adapters(&pool, json!({ "score": 75 }), &adapters)
                .await
                .expect("evaluate graph");

        assert_eq!(activated.version, 2);
        assert_eq!(
            activated.graph.nodes[0].config["manifest_hash"],
            "threshold-manifest-v1"
        );
        assert_eq!(evaluated.output, Value::Bool(true));
        assert_eq!(evaluated.trace.len(), 2);
        assert_eq!(evaluated.trace[0].node_id, "threshold");
        assert_eq!(evaluated.trace[0].input["context_ref"], "execution_input");
        assert!(evaluated.trace[0].input.get("context").is_none());
    }

    #[test]
    fn rejects_invalid_adapter_configuration_before_activation() {
        let adapters = RuleNodeAdapterRegistry::from_adapters([
            Arc::new(ThresholdAdapter) as Arc<dyn RuleNodeAdapter>
        ])
        .expect("adapter registry");
        let graph = RuleGraph {
            graph_id: DEFAULT_GRAPH_ID.to_string(),
            name: "invalid adapter config".to_string(),
            nodes: vec![RuleNode {
                id: "threshold".to_string(),
                label: "Threshold".to_string(),
                kind: "skill".to_string(),
                operation: "threshold_check".to_string(),
                config: json!({ "adapter": "threshold_check@1" }),
                input_schema: Value::Null,
                output_schema: Value::Null,
                x: 0.0,
                y: 0.0,
            }],
            edges: Vec::new(),
        };

        let error = adapters
            .validate_graph(&graph)
            .expect_err("invalid adapter configuration must fail");

        assert!(error.to_string().contains("threshold must be an integer"));
    }

    #[test]
    fn rejects_graphs_over_the_node_limit() {
        let graph = RuleGraph {
            graph_id: DEFAULT_GRAPH_ID.to_string(),
            name: "oversized".to_string(),
            nodes: (0..=MAX_RULE_GRAPH_NODES)
                .map(|index| node(&format!("node-{index}"), "output"))
                .collect(),
            edges: Vec::new(),
        };

        let error = validate_graph(&graph, &HashSet::new())
            .expect_err("oversized graph must fail validation");

        assert!(error.to_string().contains("exceeds the limit"));
    }

    #[test]
    fn adapter_kind_must_match_the_rule_node_kind() {
        let adapters = RuleNodeAdapterRegistry::from_adapters([
            Arc::new(ThresholdAdapter) as Arc<dyn RuleNodeAdapter>
        ])
        .expect("adapter registry");
        let graph = RuleGraph {
            graph_id: DEFAULT_GRAPH_ID.to_string(),
            name: "kind mismatch".to_string(),
            nodes: vec![RuleNode {
                id: "challenger".to_string(),
                label: "Challenger".to_string(),
                kind: "agent".to_string(),
                operation: "threshold_check".to_string(),
                config: json!({
                    "adapter": "threshold_check@1",
                    "threshold": 70
                }),
                input_schema: Value::Null,
                output_schema: Value::Null,
                x: 0.0,
                y: 0.0,
            }],
            edges: Vec::new(),
        };

        let error = adapters
            .validate_graph(&graph)
            .expect_err("adapter kind mismatch must fail");

        assert!(error.to_string().contains("is kind agent"));
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
