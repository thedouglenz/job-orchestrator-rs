use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio_postgres::Row;

use crate::connection::Database;
use dag_orchestrator::database::{
    DAGNode, DAGTemplateRepository, InputMappingSpec, InputSource,
};
use types::{ExecutionMode, image};

pub struct PostgresDAGTemplateRepository {
    db: Arc<Database>,
}

impl PostgresDAGTemplateRepository {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl DAGTemplateRepository for PostgresDAGTemplateRepository {
    async fn get_node(&self, node_id: &str) -> Result<DAGNode, String> {
        let client = self.db.pool.get().await.map_err(|e| {
            format!("Failed to get database client: {}", e)
        })?;

        let query = "SELECT * FROM dag_nodes WHERE id = $1";
        let row = client
            .query_one(query, &[&node_id])
            .await
            .map_err(|e| format!("Failed to query DAG node: {}", e))?;

        map_node_row(&row)
    }

    async fn get_child_nodes(&self, node_id: &str) -> Result<Vec<DAGNode>, String> {
        let client = self.db.pool.get().await.map_err(|e| {
            format!("Failed to get database client: {}", e)
        })?;

        let query = "SELECT * FROM dag_nodes WHERE parent_node_id = $1 ORDER BY node_order ASC";
        let rows = client
            .query(query, &[&node_id])
            .await
            .map_err(|e| format!("Failed to query child nodes: {}", e))?;

        let mut nodes = Vec::new();
        for row in rows {
            nodes.push(map_node_row(&row)?);
        }

        Ok(nodes)
    }
}

fn map_node_row(row: &Row) -> Result<DAGNode, String> {
    let command: Option<Vec<String>> = row
        .try_get::<_, Option<Vec<String>>>("command")
        .ok()
        .flatten();

    let args: Option<Vec<String>> = row
        .try_get::<_, Option<Vec<String>>>("args")
        .ok()
        .flatten();

    let environment_variables: Option<HashMap<String, String>> = row
        .try_get::<_, Option<String>>("environment_variables")
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok());

    let resource_overrides: Option<image::PartialResourceRequirements> = row
        .try_get::<_, Option<String>>("resource_overrides")
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .and_then(|v: serde_json::Value| serde_json::from_value(v).ok());

    let input_mapping: Option<HashMap<String, InputMappingSpec>> = row
        .try_get::<_, Option<String>>("input_mapping")
        .ok()
        .flatten()
        .and_then(|s| {
            serde_json::from_str::<serde_json::Value>(&s).ok().and_then(|v| {
                v.as_object()
                    .map(|obj| {
                        obj.iter()
                            .filter_map(|(k, v)| {
                                map_input_mapping_spec(v).map(|spec| (k.clone(), spec))
                            })
                            .collect()
                    })
            })
        });

    let execution_mode = row
        .try_get::<_, Option<String>>("execution_mode")
        .ok()
        .flatten()
        .and_then(|s| {
            if s == "sequential" {
                Some(ExecutionMode::Sequential)
            } else if s == "parallel" {
                Some(ExecutionMode::Parallel)
            } else if s.starts_with("limited:") {
                s.strip_prefix("limited:")
                    .and_then(|n| n.parse::<i32>().ok())
                    .map(ExecutionMode::Limited)
            } else {
                None
            }
        })
        .unwrap_or(ExecutionMode::Parallel);

    Ok(DAGNode {
        id: row.get("id"),
        node_name: row.get("node_name"),
        image_id: row.get("image_id"),
        command,
        args,
        environment_variables,
        resource_overrides,
        max_retries: row.get("max_retries"),
        input_mapping,
        sibling_group_id: row.get("sibling_group_id"),
        execution_mode,
        max_parallel_siblings: row.get("max_parallel_siblings"),
    })
}

fn map_input_mapping_spec(value: &serde_json::Value) -> Option<InputMappingSpec> {
    let obj = value.as_object()?;
    
    let source_str = obj.get("source")?.as_str()?;
    let source = match source_str {
        "parent" => InputSource::Parent,
        "payload" => InputSource::Payload,
        "constant" => InputSource::Constant,
        _ => return None,
    };

    Some(InputMappingSpec {
        source,
        output_key: obj.get("outputKey").and_then(|v| v.as_str()).map(|s| s.to_string()),
        payload_key: obj.get("payloadKey").and_then(|v| v.as_str()).map(|s| s.to_string()),
        value: obj.get("value").cloned(),
    })
}