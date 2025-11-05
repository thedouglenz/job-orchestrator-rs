// Common test utilities and fixtures

use executor_core::database::ImageInfo;
use types::{ExecutorConfig, JobRequest, ResourceRequirements};
use uuid::Uuid;

pub mod fixtures;

pub fn create_test_executor_config() -> ExecutorConfig {
    ExecutorConfig {
        topics: vec!["default".to_string()],
        poll_interval: 1000,
        batch_size: 5,
        kubernetes_namespace: "default".to_string(),
        instance_id: "test-executor".to_string(),
    }
}

pub fn create_test_job_request() -> JobRequest {
    JobRequest {
        topic: "default".to_string(),
        image_id: Uuid::new_v4().to_string(),
        job_name: Some("test-job".to_string()),
        command: Some(vec!["echo".to_string(), "hello".to_string()]),
        args: None,
        environment_variables: None,
        resource_overrides: None,
        priority: Some(0),
        max_retries: Some(3),
        labels: None,
        created_by: "test-user".to_string(),
        inputs: None,
    }
}

pub fn create_test_image() -> ImageInfo {
    use std::collections::HashMap;
    ImageInfo {
        id: Uuid::new_v4().to_string(),
        full_image_path: "test-repo/test-image:latest".to_string(),
        is_active: true,
        default_resources: Some(ResourceRequirements {
            cpu: "100m".to_string(),
            memory: "128Mi".to_string(),
        }),
        environment_variables: HashMap::new(),
        labels: HashMap::new(),
    }
}

pub async fn wait_for<F, Fut>(mut condition: F, timeout_ms: u64) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_millis(timeout_ms);

    while start.elapsed() < timeout {
        if condition().await {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    false
}
