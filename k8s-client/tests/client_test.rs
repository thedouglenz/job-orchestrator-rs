// Unit tests for K8s client

// Note: Full K8s client testing would require a mock Kubernetes API server
// For now, we focus on testing error handling and type conversions

use k8s_client::error::KubernetesError;
use types::K8sJobStatus;

#[test]
fn test_kubernetes_error_display() {
    let error = KubernetesError::ConfigError("Failed to load config".to_string());
    let error_str = format!("{}", error);
    assert!(error_str.contains("Failed to load config"));
}

#[test]
fn test_kubernetes_error_types() {
    let config_error = KubernetesError::ConfigError("Config error".to_string());
    match config_error {
        KubernetesError::ConfigError(_) => {}
        _ => panic!("Expected ConfigError"),
    }
    
    let create_error = KubernetesError::CreateJobError("Create failed".to_string());
    match create_error {
        KubernetesError::CreateJobError(_) => {}
        _ => panic!("Expected CreateJobError"),
    }
    
    let status_error = KubernetesError::GetJobStatusError("Status failed".to_string());
    match status_error {
        KubernetesError::GetJobStatusError(_) => {}
        _ => panic!("Expected GetJobStatusError"),
    }
}

#[test]
fn test_k8s_job_status_serialization() {
    let status = K8sJobStatus {
        active: 1,
        succeeded: 0,
        failed: 0,
        start_time: Some("2024-01-01T00:00:00Z".to_string()),
        completion_time: None,
        conditions: None,
    };
    
    // Test that it can be serialized
    let json = serde_json::to_string(&status).unwrap();
    assert!(json.contains("\"active\":1"));
    assert!(json.contains("\"succeeded\":0"));
}

// Test job completion detection
#[test]
fn test_job_status_completion() {
    let succeeded = K8sJobStatus {
        active: 0,
        succeeded: 1,
        failed: 0,
        start_time: None,
        completion_time: None,
        conditions: None,
    };
    assert_eq!(succeeded.succeeded, 1);
    
    let failed = K8sJobStatus {
        active: 0,
        succeeded: 0,
        failed: 1,
        start_time: None,
        completion_time: None,
        conditions: None,
    };
    assert_eq!(failed.failed, 1);
    
    let running = K8sJobStatus {
        active: 1,
        succeeded: 0,
        failed: 0,
        start_time: None,
        completion_time: None,
        conditions: None,
    };
    assert_eq!(running.active, 1);
}

