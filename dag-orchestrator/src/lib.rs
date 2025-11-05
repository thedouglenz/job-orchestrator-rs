pub mod database;
pub mod orchestrator;

pub use orchestrator::DAGOrchestrator;

// Implement the trait from executor-core to avoid circular dependency
use async_trait::async_trait;
use executor_core::dag_trait::DAGOrchestratorTrait;
use types::{JobOutputs, QueuedJob};

#[async_trait]
impl DAGOrchestratorTrait for DAGOrchestrator {
    fn is_dag_job(&self, job: &QueuedJob) -> bool {
        self.is_dag_job(job)
    }

    async fn handle_job_completion(
        &self,
        job: &QueuedJob,
        outputs: &Option<JobOutputs>,
    ) -> Result<(), String> {
        self.handle_job_completion(job, outputs).await
    }

    async fn handle_job_failure(&self, job: &QueuedJob, error_message: &str) -> Result<(), String> {
        self.handle_job_failure(job, error_message).await
    }
}
