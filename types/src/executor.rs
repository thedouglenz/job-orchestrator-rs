use serde::{Deserialize, Serialize};

use crate::image::ResourceRequirements;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorConfig {
    pub topics: Vec<String>,
    pub poll_interval: u64,
    pub batch_size: usize,
    pub kubernetes_namespace: String,
    pub instance_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobExecutionSpec {
    pub job_id: String,
    pub image_path: String,
    pub command: Option<Vec<String>>,
    pub args: Option<Vec<String>>,
    pub environment_variables: std::collections::HashMap<String, String>,
    pub resources: ResourceRequirements,
    pub labels: std::collections::HashMap<String, String>,
    pub namespace: String,
    pub job_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct K8sJobStatus {
    pub active: i32,
    pub succeeded: i32,
    pub failed: i32,
    pub start_time: Option<String>,
    pub completion_time: Option<String>,
    pub conditions: Option<Vec<K8sJobCondition>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct K8sJobCondition {
    pub condition_type: String,
    pub status: String,
    pub last_probe_time: Option<String>,
    pub last_transition_time: Option<String>,
    pub reason: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorCategory {
    Transient,
    Recoverable,
    Permanent,
    Unknown,
}
