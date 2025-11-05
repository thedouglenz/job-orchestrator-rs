/// Mock implementations for testing without database connections
///
/// This module provides mock implementations of all database traits that can be used
/// in unit tests without requiring actual database connections or network calls.
use async_trait::async_trait;
use dag_orchestrator::database::{
    DAGExecution, DAGExecutionRepository, DAGExecutionStats, DAGNode, DAGNodeExecution,
    DAGNodeStatus, DAGStatus, DAGTemplateRepository, NodeExecutionUpdate,
};
use executor_core::database::{
    Database, HistoryEntry, ImageInfo, ImageRegistryRepository, JobExecutionHistoryRepository,
    JobQueueRepository,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use types::{JobRequest, JobStatus, JobStatusUpdate, QueuedJob};

/// Mock database implementation that doesn't require actual database connections
#[derive(Debug, Clone)]
pub struct MockDatabase {
    /// Controls whether health_check returns true or false
    pub health_check_result: bool,
}

impl MockDatabase {
    pub fn new() -> Self {
        Self {
            health_check_result: true,
        }
    }

    /// Create a mock database that fails health checks
    pub fn with_failed_health_check() -> Self {
        Self {
            health_check_result: false,
        }
    }
}

impl Default for MockDatabase {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Database for MockDatabase {
    async fn health_check(&self) -> bool {
        self.health_check_result
    }

    async fn close(&self) {
        // No-op for mock
    }
}

/// Mock job queue repository with in-memory storage
#[derive(Clone)]
pub struct MockJobQueueRepository {
    jobs: Arc<Mutex<HashMap<String, QueuedJob>>>,
    enqueue_result: Arc<Mutex<Option<Result<String, String>>>>,
    claim_result: Arc<Mutex<Option<Result<Vec<QueuedJob>, String>>>>,
}

impl MockJobQueueRepository {
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(Mutex::new(HashMap::new())),
            enqueue_result: Arc::new(Mutex::new(None)),
            claim_result: Arc::new(Mutex::new(None)),
        }
    }

    /// Add a job to the mock storage
    pub fn add_job(&self, job: QueuedJob) {
        self.jobs.lock().unwrap().insert(job.id.clone(), job);
    }

    /// Set the result that enqueue_job should return
    pub fn set_enqueue_result(&self, result: Result<String, String>) {
        *self.enqueue_result.lock().unwrap() = Some(result);
    }

    /// Set the result that claim_jobs should return
    pub fn set_claim_result(&self, result: Result<Vec<QueuedJob>, String>) {
        *self.claim_result.lock().unwrap() = Some(result);
    }

    /// Get all stored jobs (useful for assertions)
    pub fn get_all_jobs(&self) -> Vec<QueuedJob> {
        self.jobs.lock().unwrap().values().cloned().collect()
    }

    /// Clear all stored jobs
    pub fn clear(&self) {
        self.jobs.lock().unwrap().clear();
    }
}

impl Default for MockJobQueueRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl JobQueueRepository for MockJobQueueRepository {
    async fn enqueue_job(&self, request: &JobRequest) -> Result<String, String> {
        if let Some(result) = self.enqueue_result.lock().unwrap().take() {
            return result;
        }

        // Default behavior: generate a job ID and store it
        let job_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now();
        let job = QueuedJob {
            id: job_id.clone(),
            topic: request.topic.clone(),
            image_id: request.image_id.clone(),
            job_name: request.job_name.clone(),
            command: request.command.clone(),
            args: request.args.clone(),
            environment_variables: request.environment_variables.clone(),
            resource_overrides: request.resource_overrides.clone(),
            priority: request.priority.unwrap_or(0),
            max_retries: request.max_retries.unwrap_or(3),
            created_by: request.created_by.clone(),
            labels: request.labels.clone(),
            inputs: request.inputs.clone(),
            status: JobStatus::Pending,
            retry_count: 0,
            claimed_by: None,
            claimed_at: None,
            k8s_job_name: None,
            k8s_namespace: None,
            started_at: None,
            completed_at: None,
            error_message: None,
            result_data: None,
            created_at: now,
            updated_at: now,
            outputs: None,
            dag_execution_id: None,
            dag_node_execution_id: None,
        };

        self.jobs.lock().unwrap().insert(job_id.clone(), job);
        Ok(job_id)
    }

    async fn enqueue_jobs_batch(&self, requests: &[JobRequest]) -> Result<Vec<String>, String> {
        let mut job_ids = Vec::new();
        for request in requests {
            let job_id = self.enqueue_job(request).await?;
            job_ids.push(job_id);
        }
        Ok(job_ids)
    }

    async fn claim_jobs(
        &self,
        _topics: &[String],
        _batch_size: usize,
        _executor_id: &str,
    ) -> Result<Vec<QueuedJob>, String> {
        if let Some(result) = self.claim_result.lock().unwrap().take() {
            return result;
        }

        // Default behavior: return empty list
        Ok(Vec::new())
    }

    async fn get_job(&self, job_id: &str) -> Result<QueuedJob, String> {
        self.jobs
            .lock()
            .unwrap()
            .get(job_id)
            .cloned()
            .ok_or_else(|| format!("Job not found: {}", job_id))
    }

    async fn update_job_status(&self, update: &JobStatusUpdate) -> Result<(), String> {
        let mut jobs = self.jobs.lock().unwrap();
        if let Some(job) = jobs.get_mut(&update.id) {
            job.status = update.status.clone();
            job.k8s_job_name = update.k8s_job_name.clone();
            job.claimed_by = update.claimed_by.clone();
            job.claimed_at = update.claimed_at;
            job.started_at = update.started_at;
            job.completed_at = update.completed_at;
            job.error_message = update.error_message.clone();
            job.result_data = update.result_data.clone();
            if let Some(retry_count) = update.retry_count {
                job.retry_count = retry_count;
            }
            job.updated_at = chrono::Utc::now();
            Ok(())
        } else {
            Err(format!("Job not found: {}", update.id))
        }
    }

    async fn requeue_job(&self, job_id: &str) -> Result<(), String> {
        let mut jobs = self.jobs.lock().unwrap();
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = JobStatus::Pending;
            job.claimed_by = None;
            job.claimed_at = None;
            Ok(())
        } else {
            Err(format!("Job not found: {}", job_id))
        }
    }

    async fn get_distinct_topics(&self) -> Result<Vec<String>, String> {
        let jobs = self.jobs.lock().unwrap();
        let topics: std::collections::HashSet<String> =
            jobs.values().map(|j| j.topic.clone()).collect();
        Ok(topics.into_iter().collect())
    }
}

/// Mock image registry repository
#[derive(Clone)]
pub struct MockImageRegistryRepository {
    images: Arc<Mutex<HashMap<String, ImageInfo>>>,
}

impl MockImageRegistryRepository {
    pub fn new() -> Self {
        Self {
            images: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Add an image to the mock registry
    pub fn add_image(&self, image: ImageInfo) {
        self.images.lock().unwrap().insert(image.id.clone(), image);
    }

    /// Clear all stored images
    pub fn clear(&self) {
        self.images.lock().unwrap().clear();
    }
}

impl Default for MockImageRegistryRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ImageRegistryRepository for MockImageRegistryRepository {
    async fn get_image(&self, image_id: &str) -> Result<ImageInfo, String> {
        self.images
            .lock()
            .unwrap()
            .get(image_id)
            .cloned()
            .ok_or_else(|| format!("Image not found: {}", image_id))
    }
}

/// Mock job execution history repository
#[derive(Clone)]
pub struct MockJobExecutionHistoryRepository {
    entries: Arc<Mutex<Vec<HistoryEntry>>>,
}

impl MockJobExecutionHistoryRepository {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Get all history entries (useful for assertions)
    pub fn get_all_entries(&self) -> Vec<HistoryEntry> {
        self.entries.lock().unwrap().clone()
    }

    /// Get history entries for a specific job
    pub fn get_entries_for_job(&self, job_id: &str) -> Vec<HistoryEntry> {
        self.entries
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.job_id == job_id)
            .cloned()
            .collect()
    }

    /// Clear all stored entries
    pub fn clear(&self) {
        self.entries.lock().unwrap().clear();
    }
}

impl Default for MockJobExecutionHistoryRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl JobExecutionHistoryRepository for MockJobExecutionHistoryRepository {
    async fn create_entry(&self, entry: &HistoryEntry) -> Result<(), String> {
        self.entries.lock().unwrap().push(entry.clone());
        Ok(())
    }
}

/// Mock DAG execution repository
#[derive(Clone)]
pub struct MockDAGExecutionRepository {
    executions: Arc<Mutex<HashMap<String, DAGExecution>>>,
    node_executions: Arc<Mutex<HashMap<String, DAGNodeExecution>>>,
}

impl MockDAGExecutionRepository {
    pub fn new() -> Self {
        Self {
            executions: Arc::new(Mutex::new(HashMap::new())),
            node_executions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Add a DAG execution to the mock storage
    pub fn add_execution(&self, execution: DAGExecution) {
        self.executions
            .lock()
            .unwrap()
            .insert(execution.id.clone(), execution);
    }

    /// Add a node execution to the mock storage
    pub fn add_node_execution(&self, node_execution: DAGNodeExecution) {
        self.node_executions
            .lock()
            .unwrap()
            .insert(node_execution.id.clone(), node_execution);
    }

    /// Clear all stored data
    pub fn clear(&self) {
        self.executions.lock().unwrap().clear();
        self.node_executions.lock().unwrap().clear();
    }
}

impl Default for MockDAGExecutionRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DAGExecutionRepository for MockDAGExecutionRepository {
    async fn get_execution(
        &self,
        dag_execution_id: &str,
        _include_nodes: bool,
    ) -> Result<DAGExecution, String> {
        self.executions
            .lock()
            .unwrap()
            .get(dag_execution_id)
            .cloned()
            .ok_or_else(|| format!("DAG execution not found: {}", dag_execution_id))
    }

    async fn get_node_execution(
        &self,
        node_execution_id: &str,
    ) -> Result<DAGNodeExecution, String> {
        self.node_executions
            .lock()
            .unwrap()
            .get(node_execution_id)
            .cloned()
            .ok_or_else(|| format!("Node execution not found: {}", node_execution_id))
    }

    async fn get_node_execution_by_dag_and_node(
        &self,
        dag_execution_id: &str,
        node_id: &str,
    ) -> Result<DAGNodeExecution, String> {
        self.node_executions
            .lock()
            .unwrap()
            .values()
            .find(|ne| ne.dag_execution_id == dag_execution_id && ne.dag_node_id == node_id)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "Node execution not found for DAG {} and node {}",
                    dag_execution_id, node_id
                )
            })
    }

    async fn update_execution_status(
        &self,
        dag_execution_id: &str,
        status: DAGStatus,
        _error_message: Option<&str>,
    ) -> Result<(), String> {
        let mut executions = self.executions.lock().unwrap();
        if let Some(execution) = executions.get_mut(dag_execution_id) {
            execution.status = status;
            Ok(())
        } else {
            Err(format!("DAG execution not found: {}", dag_execution_id))
        }
    }

    async fn update_node_execution_status(
        &self,
        node_execution_id: &str,
        status: DAGNodeStatus,
        _data: Option<NodeExecutionUpdate>,
    ) -> Result<(), String> {
        let mut node_executions = self.node_executions.lock().unwrap();
        if let Some(node_execution) = node_executions.get_mut(node_execution_id) {
            node_execution.status = status;
            Ok(())
        } else {
            Err(format!("Node execution not found: {}", node_execution_id))
        }
    }

    async fn get_execution_stats(
        &self,
        dag_execution_id: &str,
    ) -> Result<DAGExecutionStats, String> {
        let node_executions = self.node_executions.lock().unwrap();
        let nodes: Vec<_> = node_executions
            .values()
            .filter(|ne| ne.dag_execution_id == dag_execution_id)
            .collect();

        let total_nodes = nodes.len() as i32;
        let completed_nodes = nodes
            .iter()
            .filter(|ne| ne.status == DAGNodeStatus::Completed)
            .count() as i32;
        let failed_nodes = nodes
            .iter()
            .filter(|ne| ne.status == DAGNodeStatus::Failed)
            .count() as i32;
        let running_nodes = nodes
            .iter()
            .filter(|ne| ne.status == DAGNodeStatus::Running)
            .count() as i32;
        let pending_nodes = nodes
            .iter()
            .filter(|ne| ne.status == DAGNodeStatus::Pending)
            .count() as i32;
        let waiting_nodes = nodes
            .iter()
            .filter(|ne| ne.status == DAGNodeStatus::Waiting)
            .count() as i32;
        let skipped_nodes = nodes
            .iter()
            .filter(|ne| ne.status == DAGNodeStatus::Skipped)
            .count() as i32;

        Ok(DAGExecutionStats {
            dag_execution_id: dag_execution_id.to_string(),
            total_nodes,
            completed_nodes,
            failed_nodes,
            running_nodes,
            pending_nodes,
            waiting_nodes,
            skipped_nodes,
        })
    }
}

/// Mock DAG template repository
#[derive(Clone)]
pub struct MockDAGTemplateRepository {
    nodes: Arc<Mutex<HashMap<String, DAGNode>>>,
    child_relationships: Arc<Mutex<HashMap<String, Vec<String>>>>, // parent_id -> child_ids
}

impl MockDAGTemplateRepository {
    pub fn new() -> Self {
        Self {
            nodes: Arc::new(Mutex::new(HashMap::new())),
            child_relationships: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Add a node to the mock storage
    pub fn add_node(&self, node: DAGNode) {
        self.nodes.lock().unwrap().insert(node.id.clone(), node);
    }

    /// Add a parent-child relationship
    pub fn add_child_relationship(&self, parent_id: String, child_id: String) {
        self.child_relationships
            .lock()
            .unwrap()
            .entry(parent_id)
            .or_insert_with(Vec::new)
            .push(child_id);
    }

    /// Clear all stored data
    pub fn clear(&self) {
        self.nodes.lock().unwrap().clear();
        self.child_relationships.lock().unwrap().clear();
    }
}

impl Default for MockDAGTemplateRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DAGTemplateRepository for MockDAGTemplateRepository {
    async fn get_node(&self, node_id: &str) -> Result<DAGNode, String> {
        self.nodes
            .lock()
            .unwrap()
            .get(node_id)
            .cloned()
            .ok_or_else(|| format!("Node not found: {}", node_id))
    }

    async fn get_child_nodes(&self, node_id: &str) -> Result<Vec<DAGNode>, String> {
        let child_ids = self
            .child_relationships
            .lock()
            .unwrap()
            .get(node_id)
            .cloned()
            .unwrap_or_default();

        let nodes = self.nodes.lock().unwrap();
        let children: Vec<DAGNode> = child_ids
            .iter()
            .filter_map(|id| nodes.get(id).cloned())
            .collect();

        Ok(children)
    }
}
