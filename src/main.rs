use database;
use executor_core::database::{
    ImageRegistryRepository, JobExecutionHistoryRepository, JobQueueRepository,
};
use executor_core::error::ExecutorError;
use executor_core::JobExecutor;
use k8s_client::K8sClient;
use std::sync::Arc;
use tracing::{error, info, warn};
use types::ReadyNotification;

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

    // Check if event-driven mode is enabled
    let use_event_driven = std::env::var("EXECUTOR_EVENT_DRIVEN")
        .unwrap_or_else(|_| "false".to_string())
        .parse::<bool>()
        .unwrap_or(false);

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
        instance_id: std::env::var("EXECUTOR_INSTANCE_ID").unwrap_or_else(|_| {
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
    let job_repo: Arc<dyn JobQueueRepository> =
        Arc::new(database::PostgresJobQueueRepository::new(Arc::clone(&db)));
    let image_repo: Arc<dyn ImageRegistryRepository> = Arc::new(
        database::PostgresImageRegistryRepository::new(Arc::clone(&db)),
    );
    let history_repo: Arc<dyn JobExecutionHistoryRepository> = Arc::new(
        database::PostgresJobExecutionHistoryRepository::new(Arc::clone(&db)),
    );

    // Initialize DAG repositories
    let dag_execution_repo: Arc<dyn dag_orchestrator::database::DAGExecutionRepository> = Arc::new(
        database::PostgresDAGExecutionRepository::new(Arc::clone(&db)),
    );
    let dag_template_repo: Arc<dyn dag_orchestrator::database::DAGTemplateRepository> = Arc::new(
        database::PostgresDAGTemplateRepository::new(Arc::clone(&db)),
    );

    info!("Database connection established");

    // Create Kubernetes client
    let k8s_client = Arc::new(
        K8sClient::new()
            .await
            .map_err(|e| ExecutorError::KubernetesError(e))?,
    );

    // Set up event-driven execution if enabled
    if use_event_driven {
        info!("Starting in event-driven mode");

        // Create channel for readiness notifications
        let (ready_tx, ready_rx) = tokio::sync::mpsc::unbounded_channel::<ReadyNotification>();

        // Create DAG orchestrator with channel for event-driven notifications
        let dag_orchestrator = Arc::new(dag_orchestrator::DAGOrchestrator::new_with_channel(
            dag_execution_repo.clone(),
            dag_template_repo.clone(),
            job_repo.clone(),
            Some(ready_tx.clone()),
        ));

        // Create executor with DAG orchestrator
        let executor = JobExecutor::new_with_channel(
            config.clone(),
            job_repo.clone(),
            image_repo.clone(),
            history_repo.clone(),
            k8s_client.clone(),
            Some(
                dag_orchestrator.clone() as Arc<dyn executor_core::dag_trait::DAGOrchestratorTrait>
            ),
            Some(ready_tx.clone()),
        );

        // Bootstrap: Check for existing pending jobs on startup
        let job_repo_bootstrap = job_repo.clone();
        let ready_tx_bootstrap = ready_tx.clone();
        tokio::spawn(async move {
            // Small delay to let everything initialize
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

            // Check for pending jobs
            match job_repo_bootstrap.get_distinct_topics().await {
                Ok(topics) => {
                    if !topics.is_empty() {
                        // Estimate pending jobs (in real implementation, would query count)
                        // For now, send a notification to trigger initial poll
                        let _ = ready_tx_bootstrap.send(ReadyNotification {
                            job_count: config.batch_size,
                            priority: types::executor::Priority::Low,
                            sibling_group_id: None,
                        });
                    }
                }
                Err(e) => {
                    warn!(error = ?e, "Failed to check for pending jobs during bootstrap");
                }
            }
        });

        let executor_arc: Arc<JobExecutor> = Arc::new(executor);
        let executor_for_shutdown = Arc::clone(&executor_arc);
        let db_for_shutdown = Arc::clone(&db);

        let executor_start = Arc::clone(&executor_arc);
        let executor_handle = tokio::spawn(async move {
            if let Err(e) = executor_start.run_event_loop(ready_rx).await {
                error!(error = ?e, "Fatal error in event-driven executor loop");
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
    } else {
        info!("Starting in polling mode (legacy)");

        // Create DAG orchestrator (no channel for polling mode)
        let dag_orchestrator = Arc::new(dag_orchestrator::DAGOrchestrator::new(
            dag_execution_repo.clone(),
            dag_template_repo.clone(),
            job_repo.clone(),
        ));

        // Create executor in polling mode
        let executor = JobExecutor::new(
            config,
            job_repo,
            image_repo,
            history_repo,
            k8s_client,
            Some(
                dag_orchestrator.clone() as Arc<dyn executor_core::dag_trait::DAGOrchestratorTrait>
            ),
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
    }

    Ok(())
}
