// Unit tests using mocks - no Docker or database required
//
// These tests verify the same functionality as integration_test.rs but use mocks
// instead of real database connections

use database::mocks::{
    MockDatabase, MockImageRegistryRepository, MockJobExecutionHistoryRepository,
    MockJobQueueRepository,
};
use executor_core::database::{
    Database, HistoryEntry, ImageInfo, ImageRegistryRepository, JobExecutionHistoryRepository,
    JobQueueRepository,
};
use std::collections::HashMap;
use std::sync::Arc;
use types::{JobRequest, JobStatus, JobStatusUpdate};
use uuid::Uuid;

#[tokio::test]
async fn test_mock_database_health_check() {
    let db = MockDatabase::new();
    assert!(db.health_check().await);
}

#[tokio::test]
async fn test_mock_database_health_check_failure() {
    let db = MockDatabase::with_failed_health_check();
    assert!(!db.health_check().await);
}

#[tokio::test]
async fn test_job_enqueue_and_retrieve() {
    let job_repo = MockJobQueueRepository::new();

    let image_id = Uuid::new_v4().to_string();
    let job_request = JobRequest {
        topic: "test-topic".to_string(),
        image_id: image_id.clone(),
        job_name: Some("test-job".to_string()),
        command: Some(vec!["echo".to_string(), "hello".to_string()]),
        args: Some(vec![]),
        environment_variables: None,
        resource_overrides: None,
        priority: Some(5),
        max_retries: Some(3),
        labels: None,
        created_by: "test-user".to_string(),
        inputs: None,
    };

    // Enqueue job
    let job_id = job_repo
        .enqueue_job(&job_request)
        .await
        .expect("Failed to enqueue job");

    // Verify job ID is valid UUID
    assert!(Uuid::parse_str(&job_id).is_ok());

    // Retrieve job
    let job = job_repo.get_job(&job_id).await.expect("Failed to get job");

    // Verify job properties
    assert_eq!(job.topic, "test-topic");
    assert_eq!(job.status, JobStatus::Pending);
    assert_eq!(job.priority, 5);
    assert_eq!(job.max_retries, 3);
    assert_eq!(job.created_by, "test-user");
    assert_eq!(job.job_name, Some("test-job".to_string()));
}

#[tokio::test]
async fn test_job_status_update() {
    let job_repo = MockJobQueueRepository::new();

    let request = JobRequest {
        topic: "test".to_string(),
        image_id: Uuid::new_v4().to_string(),
        job_name: None,
        command: None,
        args: None,
        environment_variables: None,
        resource_overrides: None,
        priority: None,
        max_retries: None,
        labels: None,
        created_by: "test".to_string(),
        inputs: None,
    };

    let job_id = job_repo.enqueue_job(&request).await.unwrap();

    // Update to Running
    let update = JobStatusUpdate {
        id: job_id.clone(),
        status: JobStatus::Running,
        k8s_job_name: Some("k8s-job-123".to_string()),
        claimed_by: Some("executor-1".to_string()),
        claimed_at: Some(chrono::Utc::now()),
        started_at: Some(chrono::Utc::now()),
        completed_at: None,
        error_message: None,
        result_data: None,
        retry_count: None,
    };

    job_repo.update_job_status(&update).await.unwrap();

    // Verify update
    let job = job_repo.get_job(&job_id).await.unwrap();
    assert_eq!(job.status, JobStatus::Running);
    assert_eq!(job.k8s_job_name, Some("k8s-job-123".to_string()));
    assert_eq!(job.claimed_by, Some("executor-1".to_string()));
}

#[tokio::test]
async fn test_job_lifecycle_pending_to_completed() {
    let job_repo = MockJobQueueRepository::new();

    let request = JobRequest {
        topic: "test".to_string(),
        image_id: Uuid::new_v4().to_string(),
        job_name: Some("lifecycle-test".to_string()),
        command: Some(vec!["echo".to_string()]),
        args: Some(vec!["test".to_string()]),
        environment_variables: None,
        resource_overrides: None,
        priority: Some(0),
        max_retries: Some(3),
        labels: None,
        created_by: "test".to_string(),
        inputs: None,
    };

    let job_id = job_repo.enqueue_job(&request).await.unwrap();

    // Verify initial state
    let job = job_repo.get_job(&job_id).await.unwrap();
    assert_eq!(job.status, JobStatus::Pending);

    // Claim job
    let update = JobStatusUpdate {
        id: job_id.clone(),
        status: JobStatus::Claimed,
        claimed_by: Some("executor-1".to_string()),
        claimed_at: Some(chrono::Utc::now()),
        ..Default::default()
    };
    job_repo.update_job_status(&update).await.unwrap();

    let job = job_repo.get_job(&job_id).await.unwrap();
    assert_eq!(job.status, JobStatus::Claimed);

    // Start job
    let update = JobStatusUpdate {
        id: job_id.clone(),
        status: JobStatus::Running,
        started_at: Some(chrono::Utc::now()),
        k8s_job_name: Some("job-123".to_string()),
        ..Default::default()
    };
    job_repo.update_job_status(&update).await.unwrap();

    let job = job_repo.get_job(&job_id).await.unwrap();
    assert_eq!(job.status, JobStatus::Running);

    // Complete job
    let update = JobStatusUpdate {
        id: job_id.clone(),
        status: JobStatus::Completed,
        completed_at: Some(chrono::Utc::now()),
        ..Default::default()
    };
    job_repo.update_job_status(&update).await.unwrap();

    let job = job_repo.get_job(&job_id).await.unwrap();
    assert_eq!(job.status, JobStatus::Completed);
    assert!(job.completed_at.is_some());
}

#[tokio::test]
async fn test_job_requeue() {
    let job_repo = MockJobQueueRepository::new();

    let request = JobRequest {
        topic: "test".to_string(),
        image_id: Uuid::new_v4().to_string(),
        job_name: None,
        command: None,
        args: None,
        environment_variables: None,
        resource_overrides: None,
        priority: None,
        max_retries: None,
        labels: None,
        created_by: "test".to_string(),
        inputs: None,
    };

    let job_id = job_repo.enqueue_job(&request).await.unwrap();

    // Claim the job
    let update = JobStatusUpdate {
        id: job_id.clone(),
        status: JobStatus::Claimed,
        claimed_by: Some("executor-1".to_string()),
        claimed_at: Some(chrono::Utc::now()),
        ..Default::default()
    };
    job_repo.update_job_status(&update).await.unwrap();

    // Requeue
    job_repo.requeue_job(&job_id).await.unwrap();

    // Verify status reset
    let job = job_repo.get_job(&job_id).await.unwrap();
    assert_eq!(job.status, JobStatus::Pending);
    assert_eq!(job.claimed_by, None);
    assert_eq!(job.claimed_at, None);
}

#[tokio::test]
async fn test_batch_enqueue() {
    let job_repo = MockJobQueueRepository::new();

    let requests = vec![
        JobRequest {
            topic: "test".to_string(),
            image_id: Uuid::new_v4().to_string(),
            job_name: Some("job-1".to_string()),
            command: None,
            args: None,
            environment_variables: None,
            resource_overrides: None,
            priority: None,
            max_retries: None,
            labels: None,
            created_by: "test".to_string(),
            inputs: None,
        },
        JobRequest {
            topic: "test".to_string(),
            image_id: Uuid::new_v4().to_string(),
            job_name: Some("job-2".to_string()),
            command: None,
            args: None,
            environment_variables: None,
            resource_overrides: None,
            priority: None,
            max_retries: None,
            labels: None,
            created_by: "test".to_string(),
            inputs: None,
        },
    ];

    let job_ids = job_repo.enqueue_jobs_batch(&requests).await.unwrap();

    assert_eq!(job_ids.len(), 2);

    // Verify both jobs exist
    let job1 = job_repo.get_job(&job_ids[0]).await.unwrap();
    let job2 = job_repo.get_job(&job_ids[1]).await.unwrap();

    assert_eq!(job1.job_name, Some("job-1".to_string()));
    assert_eq!(job2.job_name, Some("job-2".to_string()));
}

#[tokio::test]
async fn test_get_distinct_topics() {
    let job_repo = MockJobQueueRepository::new();

    // Enqueue jobs in different topics
    for topic in &["topic-a", "topic-b", "topic-a", "topic-c"] {
        let request = JobRequest {
            topic: topic.to_string(),
            image_id: Uuid::new_v4().to_string(),
            job_name: None,
            command: None,
            args: None,
            environment_variables: None,
            resource_overrides: None,
            priority: None,
            max_retries: None,
            labels: None,
            created_by: "test".to_string(),
            inputs: None,
        };
        job_repo.enqueue_job(&request).await.unwrap();
    }

    let topics = job_repo.get_distinct_topics().await.unwrap();

    // Should have 3 distinct topics (topic-a appears twice but only counted once)
    assert_eq!(topics.len(), 3);
    assert!(topics.contains(&"topic-a".to_string()));
    assert!(topics.contains(&"topic-b".to_string()));
    assert!(topics.contains(&"topic-c".to_string()));
}

#[tokio::test]
async fn test_image_registry() {
    let repo = MockImageRegistryRepository::new();

    let image = ImageInfo {
        id: Uuid::new_v4().to_string(),
        full_image_path: "docker.io/library/alpine:latest".to_string(),
        is_active: true,
        environment_variables: HashMap::new(),
        default_resources: None,
        labels: HashMap::new(),
    };

    repo.add_image(image.clone());

    // Retrieve image
    let retrieved = repo.get_image(&image.id).await.unwrap();
    assert_eq!(retrieved.full_image_path, "docker.io/library/alpine:latest");
    assert!(retrieved.is_active);
}

#[tokio::test]
async fn test_image_not_found() {
    let repo = MockImageRegistryRepository::new();

    let result = repo.get_image("non-existent-id").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Image not found"));
}

#[tokio::test]
async fn test_execution_history() {
    let repo = MockJobExecutionHistoryRepository::new();

    let job_id = Uuid::new_v4().to_string();

    // Create history entries
    let entries = vec![
        HistoryEntry {
            job_id: job_id.clone(),
            job_topic: "test".to_string(),
            status: JobStatus::Pending,
            message: Some("Job created".to_string()),
            pod_logs: None,
            k8s_events: None,
            metadata: None,
        },
        HistoryEntry {
            job_id: job_id.clone(),
            job_topic: "test".to_string(),
            status: JobStatus::Running,
            message: Some("Job started".to_string()),
            pod_logs: Some("Starting container...".to_string()),
            k8s_events: None,
            metadata: None,
        },
        HistoryEntry {
            job_id: job_id.clone(),
            job_topic: "test".to_string(),
            status: JobStatus::Completed,
            message: Some("Job completed successfully".to_string()),
            pod_logs: Some("Done!".to_string()),
            k8s_events: None,
            metadata: None,
        },
    ];

    for entry in &entries {
        repo.create_entry(entry).await.unwrap();
    }

    // Retrieve history
    let history = repo.get_entries_for_job(&job_id);
    assert_eq!(history.len(), 3);
    assert_eq!(history[0].status, JobStatus::Pending);
    assert_eq!(history[1].status, JobStatus::Running);
    assert_eq!(history[2].status, JobStatus::Completed);
}

#[tokio::test]
async fn test_enqueue_error_handling() {
    let repo = MockJobQueueRepository::new();

    // Set custom error
    repo.set_enqueue_result(Err("Database connection failed".to_string()));

    let request = JobRequest {
        topic: "test".to_string(),
        image_id: Uuid::new_v4().to_string(),
        job_name: None,
        command: None,
        args: None,
        environment_variables: None,
        resource_overrides: None,
        priority: None,
        max_retries: None,
        labels: None,
        created_by: "test".to_string(),
        inputs: None,
    };

    let result = repo.enqueue_job(&request).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), "Database connection failed");
}

#[tokio::test]
async fn test_job_not_found() {
    let repo = MockJobQueueRepository::new();

    let result = repo.get_job("non-existent-id").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Job not found"));
}

#[tokio::test]
async fn test_update_nonexistent_job() {
    let repo = MockJobQueueRepository::new();

    let update = JobStatusUpdate {
        id: "non-existent-id".to_string(),
        status: JobStatus::Running,
        ..Default::default()
    };

    let result = repo.update_job_status(&update).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Job not found"));
}

#[tokio::test]
async fn test_repository_isolation() {
    let repo1 = MockJobQueueRepository::new();
    let repo2 = MockJobQueueRepository::new();

    let request = JobRequest {
        topic: "test".to_string(),
        image_id: Uuid::new_v4().to_string(),
        job_name: None,
        command: None,
        args: None,
        environment_variables: None,
        resource_overrides: None,
        priority: None,
        max_retries: None,
        labels: None,
        created_by: "test".to_string(),
        inputs: None,
    };

    let job_id = repo1.enqueue_job(&request).await.unwrap();

    // repo1 should have the job
    assert!(repo1.get_job(&job_id).await.is_ok());

    // repo2 should not have the job (different instance)
    assert!(repo2.get_job(&job_id).await.is_err());
}
