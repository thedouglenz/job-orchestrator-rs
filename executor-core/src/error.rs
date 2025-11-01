use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExecutorError {
    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Kubernetes error: {0}")]
    KubernetesError(#[from] k8s_client::error::KubernetesError),

    #[error("Job execution error: {message} (category: {category})")]
    JobExecutionError { message: String, category: String },

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Not found: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, ExecutorError>;

impl ExecutorError {
    pub fn job_execution_error(message: impl Into<String>, category: impl Into<String>) -> Self {
        ExecutorError::JobExecutionError {
            message: message.into(),
            category: category.into(),
        }
    }

    pub fn is_permanent(&self) -> bool {
        matches!(
            self,
            ExecutorError::JobExecutionError { category, .. } if category == "permanent"
        )
    }
}
