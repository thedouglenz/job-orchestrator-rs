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

/// Execution mode for DAG sibling nodes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    /// Execute siblings one at a time (current behavior)
    Sequential,
    /// Execute all siblings concurrently (new default)
    Parallel,
    /// Execute N siblings at a time (future enhancement)
    Limited(i32),
}

impl Serialize for ExecutionMode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            ExecutionMode::Sequential => serializer.serialize_str("sequential"),
            ExecutionMode::Parallel => serializer.serialize_str("parallel"),
            ExecutionMode::Limited(n) => serializer.serialize_str(&format!("limited:{}", n)),
        }
    }
}

impl<'de> Deserialize<'de> for ExecutionMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "sequential" => Ok(ExecutionMode::Sequential),
            "parallel" => Ok(ExecutionMode::Parallel),
            s if s.starts_with("limited:") => {
                let n = s
                    .strip_prefix("limited:")
                    .unwrap()
                    .parse::<i32>()
                    .map_err(serde::de::Error::custom)?;
                Ok(ExecutionMode::Limited(n))
            }
            _ => Err(serde::de::Error::custom(format!(
                "Unknown execution mode: {}",
                s
            ))),
        }
    }
}

impl Default for ExecutionMode {
    fn default() -> Self {
        ExecutionMode::Parallel
    }
}

impl ExecutionMode {
    /// Get the default execution mode
    pub fn default() -> Self {
        ExecutionMode::Parallel
    }

    /// Convert to database string representation
    pub fn to_db_string(&self) -> String {
        match self {
            ExecutionMode::Sequential => "sequential".to_string(),
            ExecutionMode::Parallel => "parallel".to_string(),
            ExecutionMode::Limited(n) => format!("limited:{}", n),
        }
    }

    /// Parse from database string representation
    pub fn from_db_string(s: &str) -> Result<Self, String> {
        match s {
            "sequential" => Ok(ExecutionMode::Sequential),
            "parallel" => Ok(ExecutionMode::Parallel),
            s if s.starts_with("limited:") => {
                let n = s
                    .strip_prefix("limited:")
                    .ok_or_else(|| "Invalid limited format".to_string())?
                    .parse::<i32>()
                    .map_err(|e| format!("Failed to parse limit: {}", e))?;
                Ok(ExecutionMode::Limited(n))
            }
            _ => Err(format!("Unknown execution mode: {}", s)),
        }
    }
}

/// Priority for job readiness notifications
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    /// High priority: New DAG started
    High,
    /// Normal priority: Sibling group ready
    Normal,
    /// Low priority: Background cleanup or recovery
    Low,
}

/// Notification sent when jobs become ready for execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadyNotification {
    /// Number of jobs ready to be claimed
    pub job_count: usize,
    /// Priority of this notification
    pub priority: Priority,
    /// Optional sibling group ID if this is a group of sibling nodes
    pub sibling_group_id: Option<String>,
}
