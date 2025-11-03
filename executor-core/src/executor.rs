use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info, warn};

use crate::database::{
    JobExecutionHistoryRepository, JobQueueRepository, ImageRegistryRepository,
    HistoryEntry,
};
use crate::error::{ExecutorError, Result};
use crate::dag_trait::DAGOrchestratorTrait;
use k8s_client::K8sClient;
use types::{
    ExecutorConfig, JobExecutionSpec, JobOutputs, JobStatus, QueuedJob,
    ReadyNotification, Priority,
};

/// Main job executor
pub struct JobExecutor {
    config: ExecutorConfig,
    job_repo: Arc<dyn JobQueueRepository>,
    image_repo: Arc<dyn ImageRegistryRepository>,
    history_repo: Arc<dyn JobExecutionHistoryRepository>,
    k8s_client: Arc<K8sClient>,
    dag_orchestrator: Option<Arc<dyn DAGOrchestratorTrait>>,
    running: Arc<tokio::sync::RwLock<bool>>,
    /// Optional channel sender for event-driven notifications (if None, uses polling)
    ready_tx: Option<tokio::sync::mpsc::UnboundedSender<ReadyNotification>>,
    /// Semaphore to limit concurrent job executions
    concurrency_limit: Arc<tokio::sync::Semaphore>,
}

impl JobExecutor {
    /// Create a new job executor
    pub fn new(
        config: ExecutorConfig,
        job_repo: Arc<dyn JobQueueRepository>,
        image_repo: Arc<dyn ImageRegistryRepository>,
        history_repo: Arc<dyn JobExecutionHistoryRepository>,
        k8s_client: Arc<K8sClient>,
        dag_orchestrator: Option<Arc<dyn DAGOrchestratorTrait>>,
    ) -> Self {
        Self::new_with_channel(
            config,
            job_repo,
            image_repo,
            history_repo,
            k8s_client,
            dag_orchestrator,
            None, // No channel = polling mode
        )
    }

    /// Create a new job executor with event-driven channel
    pub fn new_with_channel(
        config: ExecutorConfig,
        job_repo: Arc<dyn JobQueueRepository>,
        image_repo: Arc<dyn ImageRegistryRepository>,
        history_repo: Arc<dyn JobExecutionHistoryRepository>,
        k8s_client: Arc<K8sClient>,
        dag_orchestrator: Option<Arc<dyn DAGOrchestratorTrait>>,
        ready_tx: Option<tokio::sync::mpsc::UnboundedSender<ReadyNotification>>,
    ) -> Self {
        // Limit concurrent jobs (tune based on memory/CPU constraints)
        // Default to 50 concurrent jobs, but allow config override
        let max_concurrent = config.batch_size * 10; // Allow reasonable concurrency
        
        Self {
            config,
            job_repo,
            image_repo,
            history_repo,
            k8s_client,
            dag_orchestrator,
            running: Arc::new(tokio::sync::RwLock::new(false)),
            ready_tx,
            concurrency_limit: Arc::new(tokio::sync::Semaphore::new(max_concurrent)),
        }
    }

    /// Start the executor (supports both polling and event-driven modes)
    pub async fn start(&self) -> Result<()> {
        info!(
            instance_id = self.config.instance_id,
            topics = ?self.config.topics,
            poll_interval = self.config.poll_interval,
            batch_size = self.config.batch_size,
            event_driven = self.ready_tx.is_some(),
            "Starting job executor"
        );

        *self.running.write().await = true;
        
        // If we have a channel, we'll use event-driven mode (caller manages the loop)
        // Otherwise, use polling mode
        if self.ready_tx.is_none() {
            let running = self.running.clone();
            let config = self.config.clone();
            let job_repo = self.job_repo.clone();
            let image_repo = self.image_repo.clone();
            let history_repo = self.history_repo.clone();
            let k8s_client = Arc::clone(&self.k8s_client);
            let dag_orchestrator = self.dag_orchestrator.clone();

            tokio::spawn(async move {
                Self::poll_loop(
                    running,
                    config,
                    job_repo,
                    image_repo,
                    history_repo,
                    k8s_client,
                    dag_orchestrator,
                )
                .await;
            });
        }

        Ok(())
    }

    /// Start event-driven execution loop (call this instead of start() when using channels)
    pub async fn run_event_loop(
        &self,
        mut ready_rx: tokio::sync::mpsc::UnboundedReceiver<ReadyNotification>,
    ) -> Result<()> {
        info!(
            instance_id = self.config.instance_id,
            "Starting event-driven executor loop"
        );

        *self.running.write().await = true;

        loop {
            tokio::select! {
                Some(notification) = ready_rx.recv() => {
                    self.handle_ready_jobs(notification).await;
                }
                _ = tokio::signal::ctrl_c() => {
                    info!("Received shutdown signal");
                    break;
                }
            }

            // Check if we should stop
            if !*self.running.read().await {
                break;
            }
        }

        *self.running.write().await = false;
        Ok(())
    }

    /// Stop the executor
    pub async fn stop(&self) {
        info!("Stopping job executor");
        *self.running.write().await = false;
    }

    async fn poll_loop(
        running: Arc<tokio::sync::RwLock<bool>>,
        config: ExecutorConfig,
        job_repo: Arc<dyn JobQueueRepository>,
        image_repo: Arc<dyn ImageRegistryRepository>,
        history_repo: Arc<dyn JobExecutionHistoryRepository>,
        k8s_client: Arc<K8sClient>,
        dag_orchestrator: Option<Arc<dyn DAGOrchestratorTrait>>,
    ) {
        loop {
            if !*running.read().await {
                break;
            }

            match Self::poll_once(
                &config,
                &job_repo,
                &image_repo,
                &history_repo,
                &k8s_client,
                dag_orchestrator.as_ref(),
            )
            .await
            {
                Ok(_) => {}
                Err(e) => {
                    error!(error = ?e, "Error polling queue");
                }
            }

            sleep(Duration::from_millis(config.poll_interval)).await;
        }
    }

    async fn poll_once(
        config: &ExecutorConfig,
        job_repo: &Arc<dyn JobQueueRepository>,
        image_repo: &Arc<dyn ImageRegistryRepository>,
        history_repo: &Arc<dyn JobExecutionHistoryRepository>,
        k8s_client: &Arc<K8sClient>,
        dag_orchestrator: Option<&Arc<dyn DAGOrchestratorTrait>>,
    ) -> Result<()> {
        let topics = Self::get_effective_topics(config, job_repo).await?;
        
        let jobs = job_repo
            .claim_jobs(&topics, config.batch_size, &config.instance_id)
            .await
            .map_err(|e| ExecutorError::DatabaseError(e))?;

        if !jobs.is_empty() {
            info!(count = jobs.len(), topics = ?topics, "Claimed jobs from queue");

            // Process jobs in parallel
            let handles: Vec<_> = jobs
                .into_iter()
                .map(|job| {
                    let job_repo = job_repo.clone();
                    let image_repo = image_repo.clone();
                    let history_repo = history_repo.clone();
                    let k8s_client = Arc::clone(k8s_client);
                    let dag_orchestrator = dag_orchestrator.map(|d| Arc::clone(d));
                    let config = config.clone();

                    tokio::spawn(async move {
                        if let Err(e) = Self::execute_job(
                            &job,
                            &config,
                            &job_repo,
                            &image_repo,
                            &history_repo,
                            &k8s_client,
                            dag_orchestrator.as_ref(),
                        )
                        .await
                        {
                            error!(error = ?e, job_id = job.id, "Failed to execute job");
                        }
                    })
                })
                .collect();

            // Wait for all jobs to be processed (or fail)
            for handle in handles {
                let _ = handle.await;
            }
        }

        Ok(())
    }

    /// Handle ready jobs notification (non-blocking, event-driven mode)
    async fn handle_ready_jobs(&self, notification: ReadyNotification) {
        info!(
            job_count = notification.job_count,
            priority = ?notification.priority,
            sibling_group_id = ?notification.sibling_group_id,
            "Handling ready jobs notification"
        );

        let topics = match Self::get_effective_topics(&self.config, &self.job_repo).await {
            Ok(t) => t,
            Err(e) => {
                error!(error = ?e, "Failed to get effective topics");
                return;
            }
        };

        // Claim up to the notified job count, respecting batch_size limit
        let limit = notification.job_count.min(self.config.batch_size);
        
        let jobs = match self.job_repo
            .claim_jobs(&topics, limit, &self.config.instance_id)
            .await
        {
            Ok(jobs) => jobs,
            Err(e) => {
                error!(error = ?e, "Failed to claim jobs");
                return;
            }
        };

        if !jobs.is_empty() {
            info!(
                count = jobs.len(),
                topics = ?topics,
                "Claimed jobs from queue via event notification"
            );

            // Spawn each job without blocking - they run independently
            for job in jobs {
                let permit = self.concurrency_limit.clone().acquire_owned().await;
                if let Ok(permit) = permit {
                    let job_repo = self.job_repo.clone();
                    let image_repo = self.image_repo.clone();
                    let history_repo = self.history_repo.clone();
                    let k8s_client = Arc::clone(&self.k8s_client);
                    let dag_orchestrator = self.dag_orchestrator.as_ref().map(|d| Arc::clone(d));
                    let config = self.config.clone();

                    tokio::spawn(async move {
                        // Hold permit until job execution completes
                        let _permit = permit;
                        
                        if let Err(e) = Self::execute_job(
                            &job,
                            &config,
                            &job_repo,
                            &image_repo,
                            &history_repo,
                            &k8s_client,
                            dag_orchestrator.as_ref(),
                        )
                        .await
                        {
                            error!(error = ?e, job_id = job.id, "Failed to execute job");
                        }
                    });
                } else {
                    warn!(
                        job_id = job.id,
                        "Failed to acquire concurrency permit, job will be retried via polling"
                    );
                }
            }
        }
    }

    async fn get_effective_topics(
        config: &ExecutorConfig,
        job_repo: &Arc<dyn JobQueueRepository>,
    ) -> Result<Vec<String>> {
        if !config.topics.contains(&"ALL".to_string()) {
            return Ok(config.topics.clone());
        }

        job_repo
            .get_distinct_topics()
            .await
            .map_err(|e| ExecutorError::DatabaseError(e))
    }

    async fn execute_job(
        job: &QueuedJob,
        config: &ExecutorConfig,
        job_repo: &Arc<dyn JobQueueRepository>,
        image_repo: &Arc<dyn ImageRegistryRepository>,
        history_repo: &Arc<dyn JobExecutionHistoryRepository>,
        k8s_client: &Arc<K8sClient>,
        dag_orchestrator: Option<&Arc<dyn DAGOrchestratorTrait>>,
    ) -> Result<()> {
        let start_time = std::time::Instant::now();
        info!(job_id = job.id, topic = job.topic, "Executing job");

        // Record that job execution has started
        history_repo
            .create_entry(&HistoryEntry {
                job_id: job.id.clone(),
                job_topic: job.topic.clone(),
                status: JobStatus::Claimed,
                message: Some(format!("Job claimed by executor {}", config.instance_id)),
                pod_logs: None,
                k8s_events: None,
                metadata: Some({
                    let mut meta = HashMap::new();
                    meta.insert(
                        "executor_id".to_string(),
                        serde_json::Value::String(config.instance_id.clone()),
                    );
                    meta
                }),
            })
            .await
            .map_err(|e| ExecutorError::DatabaseError(e))?;

        // Get image details
        let image = image_repo
            .get_image(&job.image_id)
            .await
            .map_err(|e| ExecutorError::DatabaseError(e))?;

        if !image.is_active {
            return Err(ExecutorError::job_execution_error(
                "Image is not active",
                "permanent",
            ));
        }

        // Build environment variables including inputs
        let mut env_vars = image.environment_variables.clone();
        if let Some(job_env) = &job.environment_variables {
            env_vars.extend(job_env.clone());
        }
        env_vars.insert("JOB_ID".to_string(), job.id.clone());
        env_vars.insert("TOPIC".to_string(), job.topic.clone());

        // Add inputs as environment variables
        if let Some(inputs) = &job.inputs {
            for (key, value) in inputs {
                let env_key = key.to_uppercase();
                let env_value = match value {
                    serde_json::Value::String(s) => s.clone(),
                    _ => value.to_string(),
                };
                env_vars.insert(env_key, env_value);
            }
        }

        // Build job execution spec
        let mut resources = image.default_resources.clone().unwrap_or_else(||
            types::image::ResourceRequirements {
                cpu: "100m".to_string(),
                memory: "128Mi".to_string(),
            }
        );

        // Apply resource overrides from job
        if let Some(ref overrides) = job.resource_overrides {
            if let Some(ref cpu) = overrides.cpu {
                resources.cpu = cpu.clone();
            }
            if let Some(ref memory) = overrides.memory {
                resources.memory = memory.clone();
            }
        }

        let mut labels = image.labels.clone();
        if let Some(job_labels) = &job.labels {
            labels.extend(job_labels.clone());
        }
        labels.insert("topic".to_string(), job.topic.clone());

        let spec = JobExecutionSpec {
            job_id: job.id.clone(),
            image_path: image.full_image_path.clone(),
            command: job.command.clone(),
            args: job.args.clone(),
            environment_variables: env_vars,
            resources,
            labels,
            namespace: config.kubernetes_namespace.clone(),
            job_name: job.k8s_job_name.clone().unwrap_or_else(|| {
                format!("job-{}", &job.id[..std::cmp::min(8, job.id.len())])
            }),
        };

        // Create Kubernetes job
        k8s_client
            .create_job(&spec)
            .await
            .map_err(ExecutorError::KubernetesError)?;

        // Update job status to running
        job_repo
            .update_job_status(&types::JobStatusUpdate {
                id: job.id.clone(),
                status: JobStatus::Running,
                k8s_job_name: Some(spec.job_name.clone()),
                started_at: Some(chrono::Utc::now()),
                ..Default::default()
            })
            .await
            .map_err(|e| ExecutorError::DatabaseError(e))?;

        // Record that K8s job was created
        history_repo
            .create_entry(&HistoryEntry {
                job_id: job.id.clone(),
                job_topic: job.topic.clone(),
                status: JobStatus::Running,
                message: Some(format!("Kubernetes Job created: {}", spec.job_name)),
                pod_logs: None,
                k8s_events: None,
                metadata: Some({
                    let mut meta = HashMap::new();
                    meta.insert(
                        "k8s_job_name".to_string(),
                        serde_json::Value::String(spec.job_name.clone()),
                    );
                    meta.insert(
                        "namespace".to_string(),
                        serde_json::Value::String(spec.namespace.clone()),
                    );
                    meta.insert(
                        "image_path".to_string(),
                        serde_json::Value::String(spec.image_path.clone()),
                    );
                    meta
                }),
            })
            .await
            .map_err(|e| ExecutorError::DatabaseError(e))?;

        // Monitor job (async - don't wait)
        let job_clone = job.clone();
        let config_clone = config.clone();
        let job_repo_clone = job_repo.clone();
        let history_repo_clone = history_repo.clone();
        let k8s_client_clone = Arc::clone(k8s_client);
        let dag_orchestrator_clone = dag_orchestrator.cloned();

        tokio::spawn(async move {
            if let Err(e) = Self::monitor_job(
                &job_clone,
                &spec.job_name,
                &config_clone,
                &job_repo_clone,
                &history_repo_clone,
                &k8s_client_clone,
                dag_orchestrator_clone.as_ref(),
            )
            .await
            {
                error!(error = ?e, job_id = job_clone.id, "Error monitoring job");
            }
        });

        let duration = start_time.elapsed();
        info!(job_id = job.id, duration_ms = duration.as_millis(), "Job started successfully");

        Ok(())
    }

    async fn monitor_job(
        job: &QueuedJob,
        k8s_job_name: &str,
        config: &ExecutorConfig,
        job_repo: &Arc<dyn JobQueueRepository>,
        history_repo: &Arc<dyn JobExecutionHistoryRepository>,
        k8s_client: &Arc<K8sClient>,
        dag_orchestrator: Option<&Arc<dyn DAGOrchestratorTrait>>,
    ) -> Result<()> {
        let max_monitor_time = Duration::from_secs(24 * 60 * 60); // 24 hours
        let poll_interval = Duration::from_secs(10); // 10 seconds
        let start_time = std::time::Instant::now();

        while start_time.elapsed() < max_monitor_time {
            match k8s_client
                .get_job_status(k8s_job_name, &config.kubernetes_namespace)
                .await
            {
                Ok(status) => {
                    if status.succeeded > 0 {
                        // Job completed successfully
                        let logs = k8s_client
                            .get_pod_logs(k8s_job_name, &config.kubernetes_namespace)
                            .await
                            .unwrap_or_default();

                        job_repo
                            .update_job_status(&types::JobStatusUpdate {
                                id: job.id.clone(),
                                status: JobStatus::Completed,
                                completed_at: Some(chrono::Utc::now()),
                                ..Default::default()
                            })
                            .await
                            .map_err(|e| ExecutorError::DatabaseError(e))?;

                        let log_truncated = if logs.len() > 50000 {
                            &logs[..50000]
                        } else {
                            &logs
                        };

                        history_repo
                            .create_entry(&HistoryEntry {
                                job_id: job.id.clone(),
                                job_topic: job.topic.clone(),
                                status: JobStatus::Completed,
                                message: Some("Job completed successfully".to_string()),
                                pod_logs: Some(log_truncated.to_string()),
                                k8s_events: status.conditions.as_ref().map(|c| {
                                    serde_json::json!(c)
                                }),
                                metadata: None,
                            })
                            .await
                            .map_err(|e| ExecutorError::DatabaseError(e))?;

                        info!(job_id = job.id, "Job completed successfully");

                        // Wait for sidecar to submit outputs
                        let outputs = Self::wait_for_outputs(&job.id, job_repo).await;

                        // Handle DAG orchestration if this is a DAG job
                        if let Some(dag) = dag_orchestrator {
                            if dag.is_dag_job(job) {
                                if let Err(e) = dag.handle_job_completion(job, &outputs).await {
                                    error!(error = ?e, job_id = job.id, "Error handling DAG job completion");
                                }
                            }
                        }

                        return Ok(());
                    }

                    if status.failed > 0 {
                        // Job failed
                        let logs = k8s_client
                            .get_pod_logs(k8s_job_name, &config.kubernetes_namespace)
                            .await
                            .unwrap_or_default();

                        let should_retry = Self::should_retry_job(job, &ExecutorError::job_execution_error(
                            "Kubernetes job failed",
                            "transient",
                        ));

                        if should_retry {
                            job_repo
                                .requeue_job(&job.id)
                                .await
                                .map_err(|e| ExecutorError::DatabaseError(e))?;

                            let log_truncated = if logs.len() > 50000 {
                                &logs[..50000]
                            } else {
                                &logs
                            };

                            history_repo
                                .create_entry(&HistoryEntry {
                                    job_id: job.id.clone(),
                                    job_topic: job.topic.clone(),
                                    status: JobStatus::Failed,
                                    message: Some(format!(
                                        "Kubernetes Job failed, requeuing for retry (attempt {}/{})",
                                        job.retry_count + 1,
                                        job.max_retries
                                    )),
                                    pod_logs: Some(log_truncated.to_string()),
                                    k8s_events: status.conditions.as_ref().map(|c| {
                                        serde_json::json!(c)
                                    }),
                                    metadata: None,
                                })
                                .await
                                .map_err(|e| ExecutorError::DatabaseError(e))?;

                            info!(
                                job_id = job.id,
                                retry_count = job.retry_count + 1,
                                "Job failed, requeuing for retry"
                            );
                        } else {
                            job_repo
                                .update_job_status(&types::JobStatusUpdate {
                                    id: job.id.clone(),
                                    status: JobStatus::Failed,
                                    error_message: Some("Kubernetes job failed".to_string()),
                                    completed_at: Some(chrono::Utc::now()),
                                    result_data: Some(serde_json::json!({
                                        "logs": if logs.len() > 10000 { &logs[..10000] } else { &logs }
                                    })),
                                    ..Default::default()
                                })
                                .await
                                .map_err(|e| ExecutorError::DatabaseError(e))?;

                            let log_truncated = if logs.len() > 50000 {
                                &logs[..50000]
                            } else {
                                &logs
                            };

                            history_repo
                                .create_entry(&HistoryEntry {
                                    job_id: job.id.clone(),
                                    job_topic: job.topic.clone(),
                                    status: JobStatus::Failed,
                                    message: Some("Kubernetes Job permanently failed".to_string()),
                                    pod_logs: Some(log_truncated.to_string()),
                                    k8s_events: status.conditions.as_ref().map(|c| {
                                        serde_json::json!(c)
                                    }),
                                    metadata: None,
                                })
                                .await
                                .map_err(|e| ExecutorError::DatabaseError(e))?;

                            error!(job_id = job.id, "Job permanently failed");

                            // Handle DAG orchestration if this is a DAG job
                            if let Some(dag) = dag_orchestrator {
                                if dag.is_dag_job(job) {
                                    if let Err(e) = dag.handle_job_failure(job, "Kubernetes job failed").await {
                                        error!(error = ?e, job_id = job.id, "Error handling DAG job failure");
                                    }
                                }
                            }
                        }

                        return Ok(());
                    }

                    // Job still running, wait and check again
                    sleep(poll_interval).await;
                }
                Err(e) => {
                    error!(error = ?e, job_id = job.id, "Error monitoring job");
                    sleep(poll_interval).await;
                }
            }
        }

        // Timeout - mark job as failed
        error!(job_id = job.id, "Job monitoring timeout");

        let logs = k8s_client
            .get_pod_logs(k8s_job_name, &config.kubernetes_namespace)
            .await
            .unwrap_or_default();

        job_repo
            .update_job_status(&types::JobStatusUpdate {
                id: job.id.clone(),
                status: JobStatus::Failed,
                error_message: Some("Job monitoring timeout after 24 hours".to_string()),
                completed_at: Some(chrono::Utc::now()),
                result_data: if logs.is_empty() {
                    None
                } else {
                    Some(serde_json::json!({
                        "logs": if logs.len() > 10000 { &logs[..10000] } else { &logs }
                    }))
                },
                ..Default::default()
            })
            .await
            .map_err(|e| ExecutorError::DatabaseError(e))?;

        history_repo
            .create_entry(&HistoryEntry {
                job_id: job.id.clone(),
                job_topic: job.topic.clone(),
                status: JobStatus::Failed,
                message: Some("Job monitoring timeout after 24 hours".to_string()),
                pod_logs: if logs.is_empty() {
                    Some("No logs available".to_string())
                } else {
                    Some(if logs.len() > 50000 {
                        &logs[..50000]
                    } else {
                        &logs
                    }.to_string())
                },
                k8s_events: None,
                metadata: None,
            })
            .await
            .map_err(|e| ExecutorError::DatabaseError(e))?;

        Ok(())
    }

    fn should_retry_job(job: &QueuedJob, error: &ExecutorError) -> bool {
        if job.retry_count >= job.max_retries {
            return false;
        }

        if error.is_permanent() {
            return false;
        }

        true
    }

    async fn wait_for_outputs(
        job_id: &str,
        job_repo: &Arc<dyn JobQueueRepository>,
    ) -> Option<JobOutputs> {
        let max_wait_time = Duration::from_secs(30);
        let poll_interval = Duration::from_secs(2);
        let start_time = std::time::Instant::now();

        while start_time.elapsed() < max_wait_time {
            match job_repo.get_job(job_id).await {
                Ok(job) => {
                    if let Some(outputs) = &job.outputs {
                        if !outputs.is_empty() {
                            info!(
                                job_id,
                                output_keys = ?outputs.keys().collect::<Vec<_>>(),
                                "Outputs captured by sidecar"
                            );
                            return Some(outputs.clone());
                        }
                    }
                }
                Err(_) => {}
            }

            sleep(poll_interval).await;
        }

        warn!(job_id, "Timeout waiting for outputs from sidecar");
        None
    }
}

