use async_trait::async_trait;
use std::sync::Arc;
use tokio_postgres::Row;
use types::io::JobInputs;

use crate::connection::Database;
use dag_orchestrator::database::{
    DAGExecution, DAGExecutionRepository, DAGExecutionStats, DAGNodeExecution, DAGNodeStatus,
    DAGStatus, NodeExecutionUpdate,
};

pub struct PostgresDAGExecutionRepository {
    db: Arc<Database>,
}

impl PostgresDAGExecutionRepository {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl DAGExecutionRepository for PostgresDAGExecutionRepository {
    async fn get_execution(
        &self,
        dag_execution_id: &str,
        _include_nodes: bool,
    ) -> Result<DAGExecution, String> {
        let client = self
            .db
            .pool
            .get()
            .await
            .map_err(|e| format!("Failed to get database client: {}", e))?;

        let query = "SELECT * FROM dag_executions WHERE id = $1";
        let row = client
            .query_one(query, &[&dag_execution_id])
            .await
            .map_err(|e| format!("Failed to query DAG execution: {}", e))?;

        map_execution_row(&row)
    }

    async fn get_node_execution(
        &self,
        node_execution_id: &str,
    ) -> Result<DAGNodeExecution, String> {
        let client = self
            .db
            .pool
            .get()
            .await
            .map_err(|e| format!("Failed to get database client: {}", e))?;

        let query = "SELECT * FROM dag_node_executions WHERE id = $1";
        let row = client
            .query_one(query, &[&node_execution_id])
            .await
            .map_err(|e| format!("Failed to query DAG node execution: {}", e))?;

        map_node_execution_row(&row)
    }

    async fn get_node_execution_by_dag_and_node(
        &self,
        dag_execution_id: &str,
        node_id: &str,
    ) -> Result<DAGNodeExecution, String> {
        let client = self
            .db
            .pool
            .get()
            .await
            .map_err(|e| format!("Failed to get database client: {}", e))?;

        let query =
            "SELECT * FROM dag_node_executions WHERE dag_execution_id = $1 AND dag_node_id = $2";
        let row = client
            .query_one(query, &[&dag_execution_id, &node_id])
            .await
            .map_err(|e| format!("Failed to query DAG node execution: {}", e))?;

        map_node_execution_row(&row)
    }

    async fn update_execution_status(
        &self,
        dag_execution_id: &str,
        status: DAGStatus,
        error_message: Option<&str>,
    ) -> Result<(), String> {
        let client = self
            .db
            .pool
            .get()
            .await
            .map_err(|e| format!("Failed to get database client: {}", e))?;

        let status_str = dag_status_to_string(status);
        let mut fields = vec![
            "updated_at = NOW()".to_string(),
            format!("status = '{}'", status_str),
        ];

        if status == DAGStatus::Running {
            fields.push("started_at = COALESCE(started_at, NOW())".to_string());
        }

        if status == DAGStatus::Completed
            || status == DAGStatus::Failed
            || status == DAGStatus::Cancelled
        {
            fields.push("completed_at = COALESCE(completed_at, NOW())".to_string());
        }

        // Execute in one scope
        let result = match error_message {
            Some(err_msg) => {
                let mut params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = vec![];
                fields.push("error_message = $1".to_string());
                params.push(&err_msg);
                params.push(&dag_execution_id);

                let query = format!(
                    "UPDATE dag_executions SET {} WHERE id = $2",
                    fields.join(", ")
                );

                client
                    .execute(&query, &params)
                    .await
                    .map_err(|e| format!("Failed to update DAG execution status: {}", e))?
            }
            None => {
                let query = format!(
                    "UPDATE dag_executions SET {} WHERE id = $1",
                    fields.join(", ")
                );
                client
                    .execute(&query, &[&dag_execution_id])
                    .await
                    .map_err(|e| format!("Failed to update DAG execution status: {}", e))?
            }
        };

        if result == 0 {
            return Err(format!("DAG execution not found: {}", dag_execution_id));
        }

        Ok(())
    }

    async fn update_node_execution_status(
        &self,
        node_execution_id: &str,
        status: DAGNodeStatus,
        data: Option<NodeExecutionUpdate>,
    ) -> Result<(), String> {
        let client = self
            .db
            .pool
            .get()
            .await
            .map_err(|e| format!("Failed to get database client: {}", e))?;

        let status_str = dag_node_status_to_string(status);
        let mut fields = vec![
            "updated_at = NOW()".to_string(),
            format!("status = '{}'", status_str),
        ];

        if status == DAGNodeStatus::Running {
            fields.push("started_at = COALESCE(started_at, NOW())".to_string());
        }

        if status == DAGNodeStatus::Completed
            || status == DAGNodeStatus::Failed
            || status == DAGNodeStatus::Skipped
        {
            fields.push("completed_at = COALESCE(completed_at, NOW())".to_string());
        }

        // Build and execute everything in one block to avoid lifetime issues
        let result = {
            // Serialize JSON fields first (these are owned values)
            let inputs_json_opt = data
                .as_ref()
                .and_then(|u| u.inputs.as_ref())
                .map(|inputs| serde_json::to_string(inputs))
                .transpose()
                .map_err(|e| format!("Failed to serialize inputs: {}", e))?;

            let outputs_json_opt = data
                .as_ref()
                .and_then(|u| u.outputs.as_ref())
                .map(|outputs| serde_json::to_string(outputs))
                .transpose()
                .map_err(|e| format!("Failed to serialize outputs: {}", e))?;

            // Build fields and params together - access data fields directly
            let mut local_fields = fields;
            let mut params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = vec![];
            let mut param_count = 1;

            // Access fields directly from data to keep references valid
            match data.as_ref() {
                Some(update) => {
                    if let Some(job_id) = update.job_id.as_ref() {
                        local_fields.push(format!("job_id = ${}", param_count));
                        params.push(job_id);
                        param_count += 1;
                    }

                    if let Some(job_topic) = update.job_topic.as_ref() {
                        local_fields.push(format!("job_topic = ${}", param_count));
                        params.push(job_topic);
                        param_count += 1;
                    }

                    if let Some(inputs_json) = &inputs_json_opt {
                        local_fields.push(format!("inputs = ${}", param_count));
                        params.push(inputs_json);
                        param_count += 1;
                    }

                    if let Some(outputs_json) = &outputs_json_opt {
                        local_fields.push(format!("outputs = ${}", param_count));
                        params.push(outputs_json);
                        param_count += 1;
                    }

                    if let Some(err_msg) = update.error_message.as_ref() {
                        local_fields.push(format!("error_message = ${}", param_count));
                        params.push(err_msg);
                        param_count += 1;
                    }
                }
                None => {}
            }

            params.push(&node_execution_id);

            let query = format!(
                "UPDATE dag_node_executions SET {} WHERE id = ${}",
                local_fields.join(", "),
                param_count
            );

            client
                .execute(&query, &params)
                .await
                .map_err(|e| format!("Failed to update DAG node execution status: {}", e))?
        };

        if result == 0 {
            return Err(format!(
                "DAG node execution not found: {}",
                node_execution_id
            ));
        }

        Ok(())
    }

    async fn get_execution_stats(
        &self,
        dag_execution_id: &str,
    ) -> Result<DAGExecutionStats, String> {
        let client = self
            .db
            .pool
            .get()
            .await
            .map_err(|e| format!("Failed to get database client: {}", e))?;

        let query = r#"
            SELECT
                dag_execution_id,
                COUNT(*)::INTEGER as total_nodes,
                COUNT(*) FILTER (WHERE status = 'completed')::INTEGER as completed_nodes,
                COUNT(*) FILTER (WHERE status = 'failed')::INTEGER as failed_nodes,
                COUNT(*) FILTER (WHERE status = 'running')::INTEGER as running_nodes,
                COUNT(*) FILTER (WHERE status = 'pending')::INTEGER as pending_nodes,
                COUNT(*) FILTER (WHERE status = 'waiting')::INTEGER as waiting_nodes,
                COUNT(*) FILTER (WHERE status = 'skipped')::INTEGER as skipped_nodes
            FROM dag_node_executions
            WHERE dag_execution_id = $1
            GROUP BY dag_execution_id
        "#;

        let row = client
            .query_one(query, &[&dag_execution_id])
            .await
            .map_err(|e| format!("Failed to query DAG execution stats: {}", e))?;

        Ok(DAGExecutionStats {
            dag_execution_id: row.get("dag_execution_id"),
            total_nodes: row.get("total_nodes"),
            completed_nodes: row.get("completed_nodes"),
            failed_nodes: row.get("failed_nodes"),
            running_nodes: row.get("running_nodes"),
            pending_nodes: row.get("pending_nodes"),
            waiting_nodes: row.get("waiting_nodes"),
            skipped_nodes: row.get("skipped_nodes"),
        })
    }
}

fn map_execution_row(row: &Row) -> Result<DAGExecution, String> {
    let execution_payload: serde_json::Value = row
        .try_get::<_, Option<String>>("execution_payload")
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));

    let execution_payload_map = execution_payload
        .as_object()
        .ok_or_else(|| "execution_payload is not an object".to_string())?
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    Ok(DAGExecution {
        id: row.get("id"),
        dag_template_id: row.get("dag_template_id"),
        topic: row.get("topic"),
        status: parse_dag_status(row.get::<_, String>("status"))?,
        priority: row.get("priority"),
        execution_payload: execution_payload_map,
        created_by: row.get("created_by"),
    })
}

fn map_node_execution_row(row: &Row) -> Result<DAGNodeExecution, String> {
    let inputs: Option<JobInputs> = row
        .try_get::<_, Option<String>>("inputs")
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .and_then(|v: serde_json::Value| serde_json::from_value(v).ok());

    Ok(DAGNodeExecution {
        id: row.get("id"),
        dag_execution_id: row.get("dag_execution_id"),
        dag_node_id: row.get("dag_node_id"),
        job_id: row.get("job_id"),
        job_topic: row.get("job_topic"),
        status: parse_dag_node_status(row.get::<_, String>("status"))?,
        inputs,
    })
}

fn parse_dag_status(s: String) -> Result<DAGStatus, String> {
    match s.as_str() {
        "pending" => Ok(DAGStatus::Pending),
        "running" => Ok(DAGStatus::Running),
        "completed" => Ok(DAGStatus::Completed),
        "failed" => Ok(DAGStatus::Failed),
        "cancelled" => Ok(DAGStatus::Cancelled),
        _ => Err(format!("Unknown DAG status: {}", s)),
    }
}

fn parse_dag_node_status(s: String) -> Result<DAGNodeStatus, String> {
    match s.as_str() {
        "pending" => Ok(DAGNodeStatus::Pending),
        "waiting" => Ok(DAGNodeStatus::Waiting),
        "ready" => Ok(DAGNodeStatus::Ready),
        "running" => Ok(DAGNodeStatus::Running),
        "completed" => Ok(DAGNodeStatus::Completed),
        "failed" => Ok(DAGNodeStatus::Failed),
        "skipped" => Ok(DAGNodeStatus::Skipped),
        _ => Err(format!("Unknown DAG node status: {}", s)),
    }
}

fn dag_status_to_string(status: DAGStatus) -> &'static str {
    match status {
        DAGStatus::Pending => "pending",
        DAGStatus::Running => "running",
        DAGStatus::Completed => "completed",
        DAGStatus::Failed => "failed",
        DAGStatus::Cancelled => "cancelled",
    }
}

fn dag_node_status_to_string(status: DAGNodeStatus) -> &'static str {
    match status {
        DAGNodeStatus::Pending => "pending",
        DAGNodeStatus::Waiting => "waiting",
        DAGNodeStatus::Ready => "ready",
        DAGNodeStatus::Running => "running",
        DAGNodeStatus::Completed => "completed",
        DAGNodeStatus::Failed => "failed",
        DAGNodeStatus::Skipped => "skipped",
    }
}
