// Test data fixtures

use chrono::Utc;
use types::{JobInputs, JobStatus, QueuedJob};
use uuid::Uuid;

pub fn create_simple_job() -> QueuedJob {
    QueuedJob {
        id: Uuid::new_v4().to_string(),
        topic: "default".to_string(),
        status: JobStatus::Pending,
        image_id: Uuid::new_v4().to_string(),
        job_name: Some("simple-job".to_string()),
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
        created_at: Utc::now(),
        updated_at: Utc::now(),
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

pub fn create_job_with_inputs() -> QueuedJob {
    let mut inputs = JobInputs::new();
    inputs.insert("input1".to_string(), serde_json::json!("value1"));
    inputs.insert("input2".to_string(), serde_json::json!(42));

    let mut job = create_simple_job();
    job.inputs = Some(inputs);
    job
}

pub fn create_dag_job(dag_execution_id: &str, dag_node_execution_id: &str) -> QueuedJob {
    let mut job = create_simple_job();
    job.dag_execution_id = Some(dag_execution_id.to_string());
    job.dag_node_execution_id = Some(dag_node_execution_id.to_string());
    job
}

pub fn create_high_priority_job() -> QueuedJob {
    let mut job = create_simple_job();
    job.priority = 100;
    job
}

pub fn create_retry_job(retry_count: i32) -> QueuedJob {
    let mut job = create_simple_job();
    job.retry_count = retry_count;
    job
}

pub fn create_failing_job() -> QueuedJob {
    let mut job = create_simple_job();
    job.command = Some(vec!["false".to_string()]); // Command that fails
    job
}
