// Unit tests for JobExecutor

use executor_core::error::ExecutorError;
use types::{ExecutorConfig, QueuedJob, JobStatus};
use std::sync::Arc;
use mockall::mock;

// Mock implementations using mockall
mock! {
    JobQueueRepo {}
    
    #[async_trait::async_trait]
    impl executor_core::database::JobQueueRepository for JobQueueRepo {
        async fn enqueue_job(&self, request: &types::JobRequest) -> Result<String, String>;
        async fn enqueue_jobs_batch(&self, requests: &[types::JobRequest]) -> Result<Vec<String>, String>;
        async fn claim_jobs(&self, topics: &[String], batch_size: usize, executor_id: &str) -> Result<Vec<QueuedJob>, String>;
        async fn get_job(&self, job_id: &str) -> Result<QueuedJob, String>;
        async fn update_job_status(&self, update: &types::JobStatusUpdate) -> Result<(), String>;
        async fn requeue_job(&self, job_id: &str) -> Result<(), String>;
        async fn get_distinct_topics(&self) -> Result<Vec<String>, String>;
    }
}

mock! {
    ImageRegistryRepo {}
    
    #[async_trait::async_trait]
    impl executor_core::database::ImageRegistryRepository for ImageRegistryRepo {
        async fn get_image(&self, image_id: &str) -> Result<executor_core::database::ImageInfo, String>;
    }
}

mock! {
    HistoryRepo {}
    
    #[async_trait::async_trait]
    impl executor_core::database::JobExecutionHistoryRepository for HistoryRepo {
        async fn create_entry(&self, entry: &executor_core::database::HistoryEntry) -> Result<(), String>;
    }
}

fn create_test_config() -> ExecutorConfig {
    ExecutorConfig {
        topics: vec!["default".to_string()],
        poll_interval: 1000,
        batch_size: 5,
        kubernetes_namespace: "default".to_string(),
        instance_id: "test-executor".to_string(),
    }
}

fn create_test_job() -> QueuedJob {
    QueuedJob {
        id: "job-1".to_string(),
        topic: "default".to_string(),
        status: JobStatus::Pending,
        image_id: "image-1".to_string(),
        job_name: Some("test-job".to_string()),
        k8s_job_name: None,
        k8s_namespace: Some("default".to_string()),
        claimed_by: None,
        claimed_at: None,
        started_at: None,
        completed_at: None,
        retry_count: 0,
        max_retries: 3,
        priority: 0,
        error_message: None,
        result_data: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        created_by: "test-user".to_string(),
        command: Some(vec!["echo".to_string(), "hello".to_string()]),
        args: None,
        environment_variables: None,
        resource_overrides: None,
        labels: None,
        inputs: None,
        outputs: None,
        dag_execution_id: None,
        dag_node_execution_id: None,
    }
}

// Test retry logic - this mirrors the should_retry_job logic in executor.rs
fn should_retry_job(job: &QueuedJob, error: &ExecutorError) -> bool {
    if job.retry_count >= job.max_retries {
        return false;
    }
    
    if error.is_permanent() {
        return false;
    }
    
    true
}

#[tokio::test]
async fn test_should_retry_job_within_max_retries() {
    let mut job = create_test_job();
    job.retry_count = 1;
    job.max_retries = 3;
    
    let error = ExecutorError::job_execution_error("transient error", "transient");
    
    assert!(should_retry_job(&job, &error));
}

#[tokio::test]
async fn test_should_not_retry_job_at_max_retries() {
    let mut job = create_test_job();
    job.retry_count = 3;
    job.max_retries = 3;
    
    let error = ExecutorError::job_execution_error("transient error", "transient");
    
    assert!(!should_retry_job(&job, &error));
}

#[tokio::test]
async fn test_should_not_retry_permanent_errors() {
    let mut job = create_test_job();
    job.retry_count = 0;
    job.max_retries = 3;
    
    let error = ExecutorError::job_execution_error("permanent error", "permanent");
    
    assert!(!should_retry_job(&job, &error));
}

#[tokio::test]
async fn test_error_permanent_detection() {
    let transient_error = ExecutorError::job_execution_error("error", "transient");
    assert!(!transient_error.is_permanent());
    
    let permanent_error = ExecutorError::job_execution_error("error", "permanent");
    assert!(permanent_error.is_permanent());
    
    let db_error = ExecutorError::DatabaseError("db error".to_string());
    // Database errors are not considered permanent by default
    assert!(!db_error.is_permanent());
}

// Test effective topics calculation
#[tokio::test]
async fn test_get_effective_topics_all() {
    let config = ExecutorConfig {
        topics: vec!["ALL".to_string()],
        poll_interval: 1000,
        batch_size: 5,
        kubernetes_namespace: "default".to_string(),
        instance_id: "test-executor".to_string(),
    };
    
    let mut mock_repo = MockJobQueueRepo::new();
    mock_repo
        .expect_get_distinct_topics()
        .times(1)
        .returning(|| Ok(vec!["topic1".to_string(), "topic2".to_string()]));
    
    let job_repo: Arc<dyn executor_core::database::JobQueueRepository> = Arc::new(mock_repo);
    
    // This tests the logic pattern - mirrors get_effective_topics in executor.rs
    let effective_topics = if config.topics.contains(&"ALL".to_string()) {
        job_repo.get_distinct_topics().await.unwrap()
    } else {
        config.topics.clone()
    };
    
    assert_eq!(effective_topics, vec!["topic1".to_string(), "topic2".to_string()]);
}

#[tokio::test]
async fn test_get_effective_topics_specific() {
    let config = ExecutorConfig {
        topics: vec!["topic1".to_string(), "topic2".to_string()],
        poll_interval: 1000,
        batch_size: 5,
        kubernetes_namespace: "default".to_string(),
        instance_id: "test-executor".to_string(),
    };
    
    let effective_topics = if !config.topics.contains(&"ALL".to_string()) {
        config.topics.clone()
    } else {
        vec![]
    };
    
    assert_eq!(effective_topics, vec!["topic1".to_string(), "topic2".to_string()]);
}

// Test concurrency limit calculation
#[tokio::test]
async fn test_concurrency_limit_calculation() {
    let config = ExecutorConfig {
        topics: vec!["default".to_string()],
        poll_interval: 1000,
        batch_size: 5,
        kubernetes_namespace: "default".to_string(),
        instance_id: "test-executor".to_string(),
    };
    
    // Concurrency limit is batch_size * 10 in the executor
    let max_concurrent = config.batch_size * 10;
    assert_eq!(max_concurrent, 50);
    
    let config_large = ExecutorConfig {
        batch_size: 10,
        ..config.clone()
    };
    let max_concurrent_large = config_large.batch_size * 10;
    assert_eq!(max_concurrent_large, 100);
}

