use async_trait::async_trait;
use executor_core::database::JobQueueRepository;
use std::collections::HashMap;
use std::sync::Arc;
use types::{JobRequest, JobStatusUpdate, QueuedJob, JobStatus};

use crate::connection::Database;

pub struct PostgresJobQueueRepository {
    db: Arc<Database>,
}

impl PostgresJobQueueRepository {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl JobQueueRepository for PostgresJobQueueRepository {
    async fn enqueue_job(&self, request: &JobRequest) -> Result<String, String> {
        let query = r#"
            INSERT INTO job_queue (
                topic, image_id, job_name, command, args,
                environment_variables, resource_overrides, priority,
                max_retries, created_by, labels, inputs
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            RETURNING id
        "#;

        let env_vars_json = request.environment_variables.as_ref()
            .map(|v| serde_json::to_string(v))
            .transpose()
            .map_err(|e| format!("Failed to serialize env vars: {}", e))?;

        let resource_overrides_json = request.resource_overrides.as_ref()
            .map(|v| serde_json::to_string(v))
            .transpose()
            .map_err(|e| format!("Failed to serialize resource overrides: {}", e))?;

        let labels_json = request.labels.as_ref()
            .map(|v| serde_json::to_string(v))
            .transpose()
            .map_err(|e| format!("Failed to serialize labels: {}", e))?;

        let inputs_json = request.inputs.as_ref()
            .map(|v| serde_json::to_string(v))
            .transpose()
            .map_err(|e| format!("Failed to serialize inputs: {}", e))?;

        let client = self.db.pool.get().await
            .map_err(|e| format!("Failed to get database client: {}", e))?;

        let rows = client
            .query(
                query,
                &[
                    &request.topic,
                    &request.image_id,
                    &request.job_name,
                    &request.command,
                    &request.args,
                    &env_vars_json,
                    &resource_overrides_json,
                    &(request.priority.unwrap_or(0) as i32),
                    &(request.max_retries.unwrap_or(3) as i32),
                    &request.created_by,
                    &labels_json,
                    &inputs_json,
                ],
            )
            .await
            .map_err(|e| format!("Failed to enqueue job: {}", e))?;

        if rows.is_empty() {
            return Err("No job ID returned".to_string());
        }

        Ok(rows[0].get::<_, uuid::Uuid>("id").to_string())
    }

    async fn claim_jobs(
        &self,
        topics: &[String],
        batch_size: usize,
        executor_id: &str,
    ) -> Result<Vec<QueuedJob>, String> {
        let query = r#"
            UPDATE job_queue
            SET
                status = 'claimed',
                claimed_by = $1,
                claimed_at = NOW(),
                updated_at = NOW()
            WHERE id IN (
                SELECT id
                FROM job_queue
                WHERE
                    status = 'pending'
                    AND topic = ANY($2)
                    AND archived = false
                ORDER BY priority DESC, created_at ASC
                FOR UPDATE SKIP LOCKED
                LIMIT $3
            )
            RETURNING *
        "#;

        let client = self.db.pool.get().await
            .map_err(|e| format!("Failed to get database client: {}", e))?;

        let rows = client
            .query(query, &[&executor_id, &topics, &(batch_size as i32)])
            .await
            .map_err(|e| format!("Failed to claim jobs: {}", e))?;

        let mut jobs = Vec::new();
        for row in rows {
            jobs.push(map_row_to_queued_job(&row)?);
        }

        Ok(jobs)
    }

    async fn get_job(&self, job_id: &str) -> Result<QueuedJob, String> {
        let query = "SELECT * FROM job_queue WHERE id = $1";
        
        let client = self.db.pool.get().await
            .map_err(|e| format!("Failed to get database client: {}", e))?;

        let rows = client
            .query(query, &[&job_id])
            .await
            .map_err(|e| format!("Failed to get job: {}", e))?;

        if rows.is_empty() {
            return Err(format!("Job not found: {}", job_id));
        }

        map_row_to_queued_job(&rows[0])
    }

    async fn update_job_status(&self, update: &JobStatusUpdate) -> Result<(), String> {
        let mut fields = vec!["updated_at = NOW()".to_string()];
        let mut params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = vec![];
        let mut param_count = 1;

        fields.push(format!("status = ${}", param_count));
        let status_str = match update.status {
            JobStatus::Pending => "pending",
            JobStatus::Claimed => "claimed",
            JobStatus::Running => "running",
            JobStatus::Completed => "completed",
            JobStatus::Failed => "failed",
            JobStatus::Cancelled => "cancelled",
        };
        params.push(&status_str);
        param_count += 1;

        if let Some(ref k8s_job_name) = update.k8s_job_name {
            fields.push(format!("k8s_job_name = ${}", param_count));
            params.push(k8s_job_name);
            param_count += 1;
        }

        if let Some(ref started_at) = update.started_at {
            fields.push(format!("started_at = ${}", param_count));
            params.push(started_at);
            param_count += 1;
        }

        if let Some(ref completed_at) = update.completed_at {
            fields.push(format!("completed_at = ${}", param_count));
            params.push(completed_at);
            param_count += 1;
        }

        if let Some(ref error_message) = update.error_message {
            fields.push(format!("error_message = ${}", param_count));
            params.push(error_message);
            param_count += 1;
        }

        let result_json_opt = if let Some(ref result_data) = update.result_data {
            Some(serde_json::to_string(result_data)
                .map_err(|e| format!("Failed to serialize result data: {}", e))?)
        } else {
            None
        };
        
        if let Some(ref result_json) = result_json_opt {
            fields.push(format!("result_data = ${}", param_count));
            params.push(result_json);
            param_count += 1;
        }

        params.push(&update.id);

        let query = format!(
            "UPDATE job_queue SET {} WHERE id = ${}",
            fields.join(", "),
            param_count
        );

        let client = self.db.pool.get().await
            .map_err(|e| format!("Failed to get database client: {}", e))?;

        let rows_affected = client
            .execute(&query, &params)
            .await
            .map_err(|e| format!("Failed to update job status: {}", e))?;

        if rows_affected == 0 {
            return Err(format!("Job not found: {}", update.id));
        }

        Ok(())
    }

    async fn requeue_job(&self, job_id: &str) -> Result<(), String> {
        let query = r#"
            UPDATE job_queue
            SET
                status = 'pending',
                retry_count = retry_count + 1,
                claimed_by = NULL,
                claimed_at = NULL,
                updated_at = NOW()
            WHERE id = $1
        "#;

        let client = self.db.pool.get().await
            .map_err(|e| format!("Failed to get database client: {}", e))?;

        let rows_affected = client
            .execute(query, &[&job_id])
            .await
            .map_err(|e| format!("Failed to requeue job: {}", e))?;

        if rows_affected == 0 {
            return Err(format!("Job not found: {}", job_id));
        }

        Ok(())
    }

    async fn get_distinct_topics(&self) -> Result<Vec<String>, String> {
        let query = "SELECT DISTINCT topic FROM job_queue WHERE status = 'pending'";
        
        let client = self.db.pool.get().await
            .map_err(|e| format!("Failed to get database client: {}", e))?;

        let rows = client
            .query(query, &[])
            .await
            .map_err(|e| format!("Failed to get distinct topics: {}", e))?;

        Ok(rows
            .iter()
            .filter_map(|row| row.get::<_, Option<String>>(0))
            .collect())
    }
}

fn map_row_to_queued_job(row: &tokio_postgres::Row) -> Result<QueuedJob, String> {
    // Handle timestamps - PostgreSQL returns TIMESTAMP WITH TIME ZONE which tokio-postgres gives as chrono::DateTime<Utc>
    let claimed_at: Option<chrono::DateTime<chrono::Utc>> = row
        .try_get::<_, Option<chrono::DateTime<chrono::Utc>>>("claimed_at")
        .ok()
        .flatten();
    
    let started_at: Option<chrono::DateTime<chrono::Utc>> = row
        .try_get::<_, Option<chrono::DateTime<chrono::Utc>>>("started_at")
        .ok()
        .flatten();
    
    let completed_at: Option<chrono::DateTime<chrono::Utc>> = row
        .try_get::<_, Option<chrono::DateTime<chrono::Utc>>>("completed_at")
        .ok()
        .flatten();
    
    let created_at: chrono::DateTime<chrono::Utc> = row
        .try_get::<_, chrono::DateTime<chrono::Utc>>("created_at")
        .map_err(|e| format!("Failed to get created_at: {}", e))?;
    
    let updated_at: chrono::DateTime<chrono::Utc> = row
        .try_get::<_, chrono::DateTime<chrono::Utc>>("updated_at")
        .map_err(|e| format!("Failed to get updated_at: {}", e))?;

    // Parse JSONB columns
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

    let resource_overrides: Option<types::image::PartialResourceRequirements> = row
        .try_get::<_, Option<String>>("resource_overrides")
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .and_then(|v: serde_json::Value| serde_json::from_value(v).ok());

    let labels: Option<HashMap<String, String>> = row
        .try_get::<_, Option<String>>("labels")
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok());

    let inputs: Option<types::io::JobInputs> = row
        .try_get::<_, Option<String>>("inputs")
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .and_then(|v: serde_json::Value| serde_json::from_value(v).ok());

    let outputs: Option<types::io::JobOutputs> = row
        .try_get::<_, Option<String>>("outputs")
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .and_then(|v: serde_json::Value| serde_json::from_value(v).ok());

    let result_data: Option<serde_json::Value> = row
        .try_get::<_, Option<String>>("result_data")
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok());

    Ok(QueuedJob {
        id: row.get("id"),
        topic: row.get("topic"),
        status: parse_job_status(row.get::<_, String>("status"))?,
        image_id: row.get("image_id"),
        job_name: row.get("job_name"),
        k8s_job_name: row.get("k8s_job_name"),
        k8s_namespace: row.try_get("k8s_namespace").ok(),
        claimed_by: row.get("claimed_by"),
        claimed_at,
        started_at,
        completed_at,
        retry_count: row.get("retry_count"),
        max_retries: row.get("max_retries"),
        priority: row.get("priority"),
        error_message: row.get("error_message"),
        result_data,
        created_at,
        updated_at,
        created_by: row.get("created_by"),
        command,
        args,
        environment_variables,
        resource_overrides,
        labels,
        inputs,
        outputs,
        dag_execution_id: row.get("dag_execution_id"),
        dag_node_execution_id: row.get("dag_node_execution_id"),
    })
}

fn parse_job_status(s: String) -> Result<JobStatus, String> {
    match s.as_str() {
        "pending" => Ok(JobStatus::Pending),
        "claimed" => Ok(JobStatus::Claimed),
        "running" => Ok(JobStatus::Running),
        "completed" => Ok(JobStatus::Completed),
        "failed" => Ok(JobStatus::Failed),
        "cancelled" => Ok(JobStatus::Cancelled),
        _ => Err(format!("Unknown job status: {}", s)),
    }
}

