use async_trait::async_trait;
use types::{JobOutputs, QueuedJob};

/// Trait for DAG orchestrator to avoid circular dependency
#[async_trait]
pub trait DAGOrchestratorTrait: Send + Sync {
    fn is_dag_job(&self, job: &QueuedJob) -> bool;
    async fn handle_job_completion(
        &self,
        job: &QueuedJob,
        outputs: &Option<JobOutputs>,
    ) -> Result<(), String>;
    async fn handle_job_failure(&self, job: &QueuedJob, error_message: &str) -> Result<(), String>;
}
