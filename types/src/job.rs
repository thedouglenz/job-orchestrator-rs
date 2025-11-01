use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Pending,
    Claimed,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRequest {
    pub topic: String,
    pub image_id: String,
    pub job_name: Option<String>,
    pub command: Option<Vec<String>>,
    pub args: Option<Vec<String>>,
    pub environment_variables: Option<std::collections::HashMap<String, String>>,
    pub resource_overrides: Option<crate::image::PartialResourceRequirements>,
    pub priority: Option<i32>,
    pub max_retries: Option<i32>,
    pub labels: Option<std::collections::HashMap<String, String>>,
    pub created_by: String,
    pub inputs: Option<crate::io::JobInputs>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobStatusInfo {
    pub id: String,
    pub topic: String,
    pub status: JobStatus,
    pub image_id: String,
    pub job_name: Option<String>,
    pub k8s_job_name: Option<String>,
    pub k8s_namespace: Option<String>,
    pub claimed_by: Option<String>,
    pub claimed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub retry_count: i32,
    pub max_retries: i32,
    pub priority: i32,
    pub error_message: Option<String>,
    pub result_data: Option<serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_by: String,
    pub command: Option<Vec<String>>,
    pub args: Option<Vec<String>>,
    pub environment_variables: Option<std::collections::HashMap<String, String>>,
    pub resource_overrides: Option<crate::image::PartialResourceRequirements>,
    pub labels: Option<std::collections::HashMap<String, String>>,
    pub inputs: Option<crate::io::JobInputs>,
    pub outputs: Option<crate::io::JobOutputs>,
    pub dag_execution_id: Option<String>,
    pub dag_node_execution_id: Option<String>,
}

pub type QueuedJob = JobStatusInfo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobStatusUpdate {
    pub id: String,
    pub status: JobStatus,
    pub k8s_job_name: Option<String>,
    pub claimed_by: Option<String>,
    pub claimed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub error_message: Option<String>,
    pub result_data: Option<serde_json::Value>,
    pub retry_count: Option<i32>,
}

impl Default for JobStatusUpdate {
    fn default() -> Self {
        Self {
            id: String::new(),
            status: JobStatus::Pending,
            k8s_job_name: None,
            claimed_by: None,
            claimed_at: None,
            started_at: None,
            completed_at: None,
            error_message: None,
            result_data: None,
            retry_count: None,
        }
    }
}
