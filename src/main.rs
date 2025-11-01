use executor_core::JobExecutor;
use executor_core::database::{
    JobExecutionHistoryRepository, JobQueueRepository, ImageRegistryRepository,
};
use executor_core::error::ExecutorError;
use k8s_client::K8sClient;
use database;
use std::sync::Arc;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // Parse configuration from environment
    let topics: Vec<String> = std::env::var("EXECUTOR_TOPICS")
        .unwrap_or_else(|_| "default".to_string())
        .split(',')
        .map(|t| t.trim().to_string())
        .collect();

    let config = types::ExecutorConfig {
        topics,
        poll_interval: std::env::var("EXECUTOR_POLL_INTERVAL")
            .unwrap_or_else(|_| "5000".to_string())
            .parse()
            .unwrap_or(5000),
        batch_size: std::env::var("EXECUTOR_BATCH_SIZE")
            .unwrap_or_else(|_| "5".to_string())
            .parse()
            .unwrap_or(5),
        kubernetes_namespace: std::env::var("K8S_NAMESPACE")
            .unwrap_or_else(|_| "default".to_string()),
        instance_id: std::env::var("EXECUTOR_INSTANCE_ID")
            .unwrap_or_else(|_| {
                hostname::get()
                    .map(|h| format!("executor-{}", h.to_string_lossy()))
                    .unwrap_or_else(|_| "executor-unknown".to_string())
            }),
    };

    info!(?config, "Executor configuration loaded");

    // Initialize database
    let db = Arc::new(
        database::Database::new(database::DatabaseConfig::default())
            .map_err(|e| format!("Failed to create database: {}", e))?,
    );

    // Test database connection
    let db_healthy = db.health_check().await;
    if !db_healthy {
        error!("Failed to connect to database");
        std::process::exit(1);
    }

    // Initialize repositories
    let job_repo: Arc<dyn JobQueueRepository> = Arc::new(database::PostgresJobQueueRepository::new(Arc::clone(&db)));
    let image_repo: Arc<dyn ImageRegistryRepository> = Arc::new(database::PostgresImageRegistryRepository::new(Arc::clone(&db)));
    let history_repo: Arc<dyn JobExecutionHistoryRepository> =
        Arc::new(database::PostgresJobExecutionHistoryRepository::new(Arc::clone(&db)));

    info!("Database connection established");

    // Create Kubernetes client
    let k8s_client = Arc::new(
        K8sClient::new()
            .await
            .map_err(|e| ExecutorError::KubernetesError(e))?,
    );

    // Create executor  
    let executor = JobExecutor::new(
        config,
        job_repo,
        image_repo,
        history_repo,
        k8s_client, // Already Arc<K8sClient>
        None, // DAG orchestrator - can be added later
    );

    // Start executor
    let executor_arc: Arc<JobExecutor> = Arc::new(executor);
    let executor_for_shutdown = Arc::clone(&executor_arc);
    let db_for_shutdown: Arc<database::Database> = Arc::clone(&db);
    
    let executor_start = Arc::clone(&executor_arc);
    let executor_handle = tokio::spawn(async move {
        if let Err(e) = executor_start.start().await {
            error!(error = ?e, "Fatal error starting executor");
            std::process::exit(1);
        }
    });
    
    tokio::select! {
        _ = executor_handle => {}
        _ = tokio::signal::ctrl_c() => {
            info!("Received SIGINT/SIGTERM, shutting down gracefully...");
            executor_for_shutdown.stop().await;
            db_for_shutdown.close().await;
        }
    }

    Ok(())
}
