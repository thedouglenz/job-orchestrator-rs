// Integration tests for end-to-end job execution

use std::collections::HashMap;
use types::{JobRequest, JobStatus, ExecutorConfig};
use executor_core::database::{JobQueueRepository, ImageRegistryRepository, JobExecutionHistoryRepository};
use database::{Database, DatabaseConfig, PostgresJobQueueRepository, PostgresImageRegistryRepository, PostgresJobExecutionHistoryRepository};
use uuid::Uuid;
use std::sync::Arc;

// Note: Full integration tests require testcontainers setup
// These tests are structured but marked with #[ignore] until database migrations are available

pub mod common;

/// Setup helper that runs a test with a testcontainers database
/// This keeps docker client and container in scope properly
async fn with_test_database<F, Fut>(test_fn: F) -> Result<(), String>
where
    F: for<'a> FnOnce(Arc<Database>) -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    use testcontainers::{clients, GenericImage, core::WaitFor};
    
    let docker = clients::Cli::default();
    
    // Use generic PostgreSQL image
    let postgres_image = GenericImage::new("postgres", "15")
        .with_env_var("POSTGRES_USER", "postgres")
        .with_env_var("POSTGRES_PASSWORD", "postgres")
        .with_env_var("POSTGRES_DB", "jobqueue")
        .with_wait_for(WaitFor::message_on_stdout("database system is ready to accept connections"));
    
    let container = docker.run(postgres_image);
    
    // Get connection details
    let host = "localhost".to_string();
    let port = container.get_host_port_ipv4(5432);
    
    let config = DatabaseConfig {
        host,
        port: port as u16,
        database: "jobqueue".to_string(),
        user: "postgres".to_string(),
        password: Some("postgres".to_string()),
        max_connections: Some(10),
    };
    
    let db = Arc::new(Database::new(config)
        .map_err(|e| format!("Failed to create database: {}", e))?);
    
    // Wait for database to be ready
    let mut retries = 30;
    while retries > 0 {
        if db.health_check().await {
            break;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        retries -= 1;
    }
    
    if !db.health_check().await {
        return Err("Database health check failed after retries".to_string());
    }
    
    // Run migrations
    run_migrations(&db).await?;
    
    // Create partitions for test topics
    create_test_partitions(&db).await?;
    
    // Run the test with container and docker staying in scope
    test_fn(db).await
}

/// Create partitions for test topics
async fn create_test_partitions(db: &Arc<Database>) -> Result<(), String> {
    // Create partitions for common test topics
    // Note: PostgreSQL will use the default partition if a specific partition doesn't exist
    // But for cleaner tests, we can create specific partitions
    let partitions = vec![
        "test-topic",
        "topic-a",
        "topic-b",
        "topic-1",
        "topic-2",
        "topic-3",
    ];
    
    for topic in partitions {
        let query = format!(
            "CREATE TABLE IF NOT EXISTS job_queue_{} PARTITION OF job_queue FOR VALUES IN ('{}')",
            topic.replace("-", "_"),
            topic
        );
        // Ignore errors if partition already exists
        let _ = db.execute_sql(&query).await;
    }
    
    Ok(())
}


/// Run database migrations from SQL files
/// Loads all migrations from job-sys/packages/database/migrations/sql/ and executes them in order
async fn run_migrations(db: &Arc<Database>) -> Result<(), String> {
    // Load all migration files in order
    let migrations = vec![
        include_str!("../../job-sys/packages/database/migrations/sql/V1__create_image_registry.sql"),
        include_str!("../../job-sys/packages/database/migrations/sql/V2__create_job_queue.sql"),
        include_str!("../../job-sys/packages/database/migrations/sql/V3__create_job_execution_history.sql"),
        include_str!("../../job-sys/packages/database/migrations/sql/V4__create_dead_letter_queue.sql"),
        include_str!("../../job-sys/packages/database/migrations/sql/V5__add_job_io_columns.sql"),
        include_str!("../../job-sys/packages/database/migrations/sql/V6__create_dag_tables.sql"),
        include_str!("../../job-sys/packages/database/migrations/sql/V7__add_job_archival.sql"),
        include_str!("../../job-sys/packages/database/migrations/sql/V8__add_sibling_parallelization_fields.sql"),
    ];
    
    for (i, migration_sql) in migrations.iter().enumerate() {
        let version = i + 1;
        db.execute_sql(migration_sql).await
            .map_err(|e| format!("Failed to run migration V{}: {}", version, e))?;
    }
    
    Ok(())
}

/// Setup a test database connection using environment variables
/// Useful when you want to use an existing database instead of testcontainers
async fn setup_test_database_from_env() -> Result<Arc<Database>, String> {
    // Try to use environment variables for test database connection
    let config = DatabaseConfig::default();
    let db = Arc::new(Database::new(config)
        .map_err(|e| format!("Failed to create database: {}", e))?);
    
    if !db.health_check().await {
        return Err("Database health check failed. Ensure test database is running.".to_string());
    }
    
    // Run migrations
    run_migrations(&db).await?;
    
    Ok(db)
}

/// Create a test image in the registry using SQL
/// Helper for integration tests - inserts directly into database
/// Note: Uses SQL string formatting for test convenience
async fn create_test_image(db: &Arc<Database>, image_id: &str, name: &str, repository: &str, tag: &str) -> Result<(), String> {
    let full_image_path = format!("{}/{}:{}", repository, name, tag);
    let default_resources = serde_json::json!({
        "cpu": "100m",
        "memory": "128Mi"
    });
    
    // Use parameterized query with proper escaping
    // Since execute_sql uses batch_execute, we'll use a direct SQL with proper escaping
    let query = format!(
        r#"
        INSERT INTO image_registry (
            id, name, tag, repository, full_image_path,
            is_active, default_resources, environment_variables, labels
        ) VALUES (
            '{}'::uuid, '{}', '{}', '{}', '{}',
            true, '{}'::jsonb, '{{}}'::jsonb, '{{}}'::jsonb
        )
        ON CONFLICT (repository, name, tag) DO NOTHING
        "#,
        image_id,
        name.replace("'", "''"),  // Escape single quotes
        tag.replace("'", "''"),
        repository.replace("'", "''"),
        full_image_path.replace("'", "''"),
        default_resources.to_string().replace("'", "''"),
    );
    
    db.execute_sql(&query).await
        .map_err(|e| format!("Failed to create test image: {}", e))?;
    
    Ok(())
}

/// Test: Full job lifecycle - submit → claim → execute → complete
#[tokio::test]
#[ignore] // Ignore until Docker is available or test database is set up
async fn test_full_job_lifecycle() {
    with_test_database(|db| async move {
        let job_repo: Arc<dyn JobQueueRepository> = Arc::new(PostgresJobQueueRepository::new(Arc::clone(&db)));
        let image_repo: Arc<dyn ImageRegistryRepository> = Arc::new(PostgresImageRegistryRepository::new(Arc::clone(&db)));
        let history_repo: Arc<dyn JobExecutionHistoryRepository> = Arc::new(PostgresJobExecutionHistoryRepository::new(Arc::clone(&db)));
    
    // Create test image in registry
    let image_id = Uuid::new_v4().to_string();
    create_test_image(&db, &image_id, "test-image", "test-repo", "latest").await
        .expect("Failed to create test image");
    
    // Submit job
    let job_request = JobRequest {
        topic: "test-topic".to_string(),
        image_id: image_id.clone(),
        job_name: Some("integration-test-job".to_string()),
        command: Some(vec!["echo".to_string(), "hello".to_string()]),
        args: Some(vec![]),
        environment_variables: None,
        resource_overrides: None,
        priority: Some(0),
        max_retries: Some(3),
        labels: None,
        created_by: "integration-test".to_string(),
        inputs: None,
    };
    
    let job_id = job_repo.enqueue_job(&job_request).await.expect("Failed to enqueue job");
    
    // Verify job is pending
    let job = job_repo.get_job(&job_id).await.expect("Failed to get job");
    assert_eq!(job.status, JobStatus::Pending);
    
    // Create executor and claim job
    let config = ExecutorConfig {
        topics: vec!["test-topic".to_string()],
        poll_interval: 1000,
        batch_size: 1,
        kubernetes_namespace: "default".to_string(),
        instance_id: "test-executor".to_string(),
    };
    
    // Note: Full test would require Kubernetes client mock or test cluster
    // This structure shows what the test would look like
    // In practice, you'd mock the K8s client or use a test cluster
    
    // Assertions would verify:
    // 1. Job was claimed
    // 2. Job status updated to Running
    // 3. Kubernetes job created (if K8s client available)
    // 4. History entry created
    // 5. Job completes successfully
        
        Ok(())
    }).await.expect("Test failed");
}

/// Test: Job retry flow with failing job
#[tokio::test]
#[ignore] // Ignore until Docker is available or test database is set up
async fn test_job_retry_flow() {
    with_test_database(|db| async move {
        let job_repo: Arc<dyn JobQueueRepository> = Arc::new(PostgresJobQueueRepository::new(Arc::clone(&db)));
    
    // Create test image in registry
    let image_id = Uuid::new_v4().to_string();
    create_test_image(&db, &image_id, "test-image", "test-repo", "latest").await
        .expect("Failed to create test image");
    
    // Create job that will fail
    let job_request = JobRequest {
        topic: "test-topic".to_string(),
        image_id,
        job_name: Some("failing-job".to_string()),
        command: Some(vec!["false".to_string()]), // Command that fails
        args: Some(vec![]),
        environment_variables: None,
        resource_overrides: None,
        priority: Some(0),
        max_retries: Some(2), // Allow 2 retries
        labels: None,
        created_by: "integration-test".to_string(),
        inputs: None,
    };
    
    let job_id = job_repo.enqueue_job(&job_request).await.expect("Failed to enqueue job");
    
    // Test would verify:
    // 1. Job fails first time
    // 2. Retry count increments
    // 3. Job requeued
    // 4. After max retries, job marked as permanently failed
        
        Ok(())
    }).await.expect("Test failed");
}

/// Test: Topic filtering - executor only claims jobs from configured topics
#[tokio::test]
#[ignore] // Ignore until Docker is available or test database is set up
async fn test_topic_filtering() {
    with_test_database(|db| async move {
        let job_repo: Arc<dyn JobQueueRepository> = Arc::new(PostgresJobQueueRepository::new(Arc::clone(&db)));
    
    // Create test image in registry
    let image_id = Uuid::new_v4().to_string();
    create_test_image(&db, &image_id, "test-image", "test-repo", "latest").await
        .expect("Failed to create test image");
    
    // Create jobs in different topics
    
    let job1 = JobRequest {
        topic: "topic-a".to_string(),
        image_id: image_id.clone(),
        job_name: Some("job-a".to_string()),
        command: Some(vec!["echo".to_string(), "a".to_string()]),
        args: Some(vec![]),
        environment_variables: None,
        resource_overrides: None,
        priority: Some(0),
        max_retries: Some(3),
        labels: None,
        created_by: "test".to_string(),
        inputs: None,
    };
    
    let job2 = JobRequest {
        topic: "topic-b".to_string(),
        image_id: image_id.clone(),
        job_name: Some("job-b".to_string()),
        command: Some(vec!["echo".to_string(), "b".to_string()]),
        args: Some(vec![]),
        environment_variables: None,
        resource_overrides: None,
        priority: Some(0),
        max_retries: Some(3),
        labels: None,
        created_by: "test".to_string(),
        inputs: None,
    };
    
    let job_id1 = job_repo.enqueue_job(&job1).await.expect("Failed to enqueue job1");
    let job_id2 = job_repo.enqueue_job(&job2).await.expect("Failed to enqueue job2");
    
    // Verify jobs are enqueued
    let _job1_status = job_repo.get_job(&job_id1).await.expect("Failed to get job1");
    let _job2_status = job_repo.get_job(&job_id2).await.expect("Failed to get job2");
    
    // Executor configured for topic-a only
    let _config = ExecutorConfig {
        topics: vec!["topic-a".to_string()],
        poll_interval: 1000,
        batch_size: 10,
        kubernetes_namespace: "default".to_string(),
        instance_id: "test-executor".to_string(),
    };
    
    // Test would verify:
    // 1. Only job1 (topic-a) is claimed
    // 2. job2 (topic-b) remains pending
        
        Ok(())
    }).await.expect("Test failed");
}

/// Test: "ALL" topics mode - executor claims from all topics
#[tokio::test]
#[ignore] // Ignore until Docker is available or test database is set up
async fn test_all_topics_mode() {
    with_test_database(|db| async move {
        let job_repo: Arc<dyn JobQueueRepository> = Arc::new(PostgresJobQueueRepository::new(Arc::clone(&db)));
    
    // Create test image in registry
    let image_id = Uuid::new_v4().to_string();
    create_test_image(&db, &image_id, "test-image", "test-repo", "latest").await
        .expect("Failed to create test image");
    
    // Create jobs in multiple topics
    
    let topics = vec!["topic-1", "topic-2", "topic-3"];
    let mut job_ids = Vec::new();
    
    for topic in &topics {
        let job = JobRequest {
            topic: topic.to_string(),
            image_id: image_id.clone(),
            job_name: Some(format!("job-{}", topic)),
            command: Some(vec!["echo".to_string(), "test".to_string()]),
            args: Some(vec![]),
            environment_variables: None,
            resource_overrides: None,
            priority: Some(0),
            max_retries: Some(3),
            labels: None,
            created_by: "test".to_string(),
            inputs: None,
        };
        
        let job_id = job_repo.enqueue_job(&job).await.expect(&format!("Failed to enqueue job for {}", topic));
        job_ids.push(job_id);
    }
    
    // Executor configured for "ALL" topics
    let _config = ExecutorConfig {
        topics: vec!["ALL".to_string()],
        poll_interval: 1000,
        batch_size: 10,
        kubernetes_namespace: "default".to_string(),
        instance_id: "test-executor".to_string(),
    };
    
    // Test would verify:
    // 1. Executor gets distinct topics from database
    // 2. All jobs from all topics are claimed
        
        Ok(())
    }).await.expect("Test failed");
}

/// Test helper: Wait for job status
async fn wait_for_job_status(
    job_repo: &Arc<dyn JobQueueRepository>,
    job_id: &str,
    expected_status: JobStatus,
    timeout_seconds: u64,
) -> bool {
    use std::time::{Duration, Instant};
    
    let start = Instant::now();
    let timeout = Duration::from_secs(timeout_seconds);
    
    while start.elapsed() < timeout {
        if let Ok(job) = job_repo.get_job(job_id).await {
            if job.status == expected_status {
                return true;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    
    false
}

/// Test helper: Verify job execution history entries
async fn verify_history_entry(
    _history_repo: &Arc<dyn JobExecutionHistoryRepository>,
    _job_id: &str,
    _expected_status: JobStatus,
) -> bool {
    // In full implementation, history_repo would have a method to query history
    // For now, this is a placeholder
    // history_repo.get_history(job_id).await
    //     .map(|entries| entries.iter().any(|e| e.status == expected_status))
    //     .unwrap_or(false)
    false // Placeholder
}

