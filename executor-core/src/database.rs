use async_trait::async_trait;
use types::{
    JobRequest, JobStatusUpdate, QueuedJob, JobStatus,
};
use std::collections::HashMap;

/// Database connection trait
#[async_trait]
pub trait Database: Send + Sync {
    async fn health_check(&self) -> bool;
    async fn close(&self);
}

/// Job queue repository trait
#[async_trait]
pub trait JobQueueRepository: Send + Sync {
    async fn enqueue_job(&self, request: &JobRequest) -> Result<String, String>;
    async fn claim_jobs(&self, topics: &[String], batch_size: usize, executor_id: &str) -> Result<Vec<QueuedJob>, String>;
    async fn get_job(&self, job_id: &str) -> Result<QueuedJob, String>;
    async fn update_job_status(&self, update: &JobStatusUpdate) -> Result<(), String>;
    async fn requeue_job(&self, job_id: &str) -> Result<(), String>;
    async fn get_distinct_topics(&self) -> Result<Vec<String>, String>;
}

/// Image registry repository trait
#[async_trait]
pub trait ImageRegistryRepository: Send + Sync {
    async fn get_image(&self, image_id: &str) -> Result<ImageInfo, String>;
}

/// Image information
#[derive(Debug, Clone)]
pub struct ImageInfo {
    pub id: String,
    pub full_image_path: String,
    pub is_active: bool,
    pub environment_variables: HashMap<String, String>,
    pub default_resources: Option<types::image::ResourceRequirements>,
    pub labels: HashMap<String, String>,
}

impl ImageInfo {
    pub fn new(
        id: String,
        full_image_path: String,
        is_active: bool,
        environment_variables: HashMap<String, String>,
        default_resources: Option<types::image::ResourceRequirements>,
        labels: HashMap<String, String>,
    ) -> Self {
        Self {
            id,
            full_image_path,
            is_active,
            environment_variables,
            default_resources,
            labels,
        }
    }
}

/// Job execution history repository trait
#[async_trait]
pub trait JobExecutionHistoryRepository: Send + Sync {
    async fn create_entry(&self, entry: &HistoryEntry) -> Result<(), String>;
}

/// History entry
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub job_id: String,
    pub job_topic: String,
    pub status: JobStatus,
    pub message: Option<String>,
    pub pod_logs: Option<String>,
    pub k8s_events: Option<serde_json::Value>,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}
