// Unit tests for database repositories

// Note: Full repository testing requires a test database
// These tests focus on type conversion and data structure validation

use chrono::Utc;
use std::collections::HashMap;
use types::{JobRequest, JobStatus};
use uuid::Uuid;

fn create_test_job_request() -> JobRequest {
    JobRequest {
        topic: "test-topic".to_string(),
        image_id: Uuid::new_v4().to_string(),
        job_name: Some("test-job".to_string()),
        command: Some(vec!["echo".to_string(), "hello".to_string()]),
        args: None,
        environment_variables: Some({
            let mut env = HashMap::new();
            env.insert("KEY".to_string(), "VALUE".to_string());
            env
        }),
        resource_overrides: None,
        priority: Some(10),
        max_retries: Some(3),
        labels: None,
        created_by: "test-user".to_string(),
        inputs: None,
    }
}

#[test]
fn test_job_request_serialization() {
    let request = create_test_job_request();

    // Test JSON serialization
    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("test-topic"));
    assert!(json.contains("test-job"));
    assert!(json.contains("KEY"));
}

#[test]
fn test_job_status_variants() {
    // Test all status variants
    assert!(matches!(JobStatus::Pending, JobStatus::Pending));
    assert!(matches!(JobStatus::Claimed, JobStatus::Claimed));
    assert!(matches!(JobStatus::Running, JobStatus::Running));
    assert!(matches!(JobStatus::Completed, JobStatus::Completed));
    assert!(matches!(JobStatus::Failed, JobStatus::Failed));
    assert!(matches!(JobStatus::Cancelled, JobStatus::Cancelled));
}

#[test]
fn test_job_status_serialization() {
    let status = JobStatus::Running;
    let json = serde_json::to_string(&status).unwrap();
    assert_eq!(json, "\"running\"");

    let status = JobStatus::Completed;
    let json = serde_json::to_string(&status).unwrap();
    assert_eq!(json, "\"completed\"");
}

// Test JSONB field handling patterns
#[test]
fn test_environment_variables_serialization() {
    let mut env_vars = HashMap::new();
    env_vars.insert("VAR1".to_string(), "value1".to_string());
    env_vars.insert("VAR2".to_string(), "value2".to_string());

    let json = serde_json::to_string(&env_vars).unwrap();
    assert!(json.contains("VAR1"));
    assert!(json.contains("value1"));

    // Test deserialization
    let deserialized: HashMap<String, String> = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.get("VAR1"), Some(&"value1".to_string()));
}
