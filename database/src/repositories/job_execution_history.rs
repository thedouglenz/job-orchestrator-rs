use async_trait::async_trait;
use executor_core::database::{JobExecutionHistoryRepository, HistoryEntry};
use std::sync::Arc;

use crate::connection::Database;

pub struct PostgresJobExecutionHistoryRepository {
    db: Arc<Database>,
}

impl PostgresJobExecutionHistoryRepository {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl JobExecutionHistoryRepository for PostgresJobExecutionHistoryRepository {
    async fn create_entry(&self, entry: &HistoryEntry) -> Result<(), String> {
        let query = r#"
            INSERT INTO job_execution_history (
                job_id, job_topic, status, message, k8s_events, pod_logs, resource_usage, metadata
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#;

        let status_str = match entry.status {
            types::JobStatus::Pending => "pending",
            types::JobStatus::Claimed => "claimed",
            types::JobStatus::Running => "running",
            types::JobStatus::Completed => "completed",
            types::JobStatus::Failed => "failed",
            types::JobStatus::Cancelled => "cancelled",
        };

        let k8s_events_json = entry.k8s_events.as_ref()
            .and_then(|v| serde_json::to_string(v).ok());
        
        let metadata_json = entry.metadata.as_ref()
            .and_then(|m| serde_json::to_string(m).ok());

        let client = self.db.pool.get().await
            .map_err(|e| format!("Failed to get database client: {}", e))?;

        client
            .execute(
                query,
                &[
                    &entry.job_id,
                    &entry.job_topic,
                    &status_str,
                    &entry.message,
                    &k8s_events_json,
                    &entry.pod_logs,
                    &None::<String>, // resource_usage not in HistoryEntry yet
                    &metadata_json,
                ],
            )
            .await
            .map_err(|e| format!("Failed to create history entry: {}", e))?;

        Ok(())
    }
}
