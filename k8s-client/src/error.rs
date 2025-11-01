use thiserror::Error;

#[derive(Debug, Error)]
pub enum KubernetesError {
    #[error("Kubernetes API error: {0}")]
    ApiError(String),

    #[error("Failed to create job: {0}")]
    CreateJobError(String),

    #[error("Failed to get job status: {0}")]
    GetJobStatusError(String),

    #[error("Failed to delete job: {0}")]
    DeleteJobError(String),

    #[error("Failed to get pod logs: {0}")]
    GetPodLogsError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),
}

pub type Result<T> = std::result::Result<T, KubernetesError>;
