// Unit tests for error handling

use executor_core::error::ExecutorError;

#[test]
fn test_job_execution_error_creation() {
    let error = ExecutorError::job_execution_error("test message", "transient");

    match error {
        ExecutorError::JobExecutionError { message, category } => {
            assert_eq!(message, "test message");
            assert_eq!(category, "transient");
        }
        _ => panic!("Expected JobExecutionError"),
    }
}

#[test]
fn test_error_is_permanent() {
    let permanent = ExecutorError::job_execution_error("error", "permanent");
    assert!(permanent.is_permanent());

    let transient = ExecutorError::job_execution_error("error", "transient");
    assert!(!transient.is_permanent());

    let recoverable = ExecutorError::job_execution_error("error", "recoverable");
    assert!(!recoverable.is_permanent());
}

#[test]
fn test_database_error() {
    let error = ExecutorError::DatabaseError("Connection failed".to_string());

    match &error {
        ExecutorError::DatabaseError(msg) => {
            assert_eq!(msg, "Connection failed");
        }
        _ => panic!("Expected DatabaseError"),
    }

    // Database errors are not permanent
    assert!(!error.is_permanent());
}

#[test]
fn test_kubernetes_error_from() {
    use k8s_client::error::KubernetesError;

    let k8s_error = KubernetesError::ConfigError("Config failed".to_string());
    let executor_error = ExecutorError::from(k8s_error);

    match executor_error {
        ExecutorError::KubernetesError(_) => {}
        _ => panic!("Expected KubernetesError"),
    }
}

#[test]
fn test_error_display() {
    let error = ExecutorError::job_execution_error("Test error", "permanent");
    let error_str = format!("{}", error);
    assert!(error_str.contains("Test error"));
    assert!(error_str.contains("permanent"));
}
