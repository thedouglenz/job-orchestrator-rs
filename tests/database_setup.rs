// Database setup utilities for integration tests
//
// This module provides both mock implementations for fast unit tests
// and testcontainers-based setup for integration tests.

use database::{Database, DatabaseConfig};
use std::sync::Arc;

/// Setup mock database for unit tests (no network/docker required)
/// This is the recommended approach for most tests as it's fast and doesn't require infrastructure
pub fn setup_mock_database() -> Arc<dyn executor_core::database::Database> {
    // Import the mocks module which contains MockDatabase
    use database::mocks::MockDatabase;
    Arc::new(MockDatabase::new())
}

/// Setup mock database that fails health checks
/// Useful for testing error handling scenarios
pub fn setup_mock_database_with_failed_health_check() -> Arc<dyn executor_core::database::Database>
{
    use database::mocks::MockDatabase;
    Arc::new(MockDatabase::with_failed_health_check())
}

/// Setup test database using testcontainers
/// Returns both the database connection and the container (which keeps it alive)
///
/// WARNING: This requires Docker to be running and will create actual containers.
/// Only use this for integration tests that require real database behavior.
/// For most tests, use setup_mock_database() instead.
#[cfg(feature = "integration-tests")]
pub async fn setup_test_database_with_container() -> Result<
    (
        Arc<Database>,
        testcontainers::Container<'_, testcontainers::images::postgres::Postgres>,
    ),
    String,
> {
    use testcontainers::{clients, images::postgres::Postgres};

    let docker = clients::Cli::default();
    let postgres_image = Postgres::default()
        .with_user("postgres")
        .with_password("postgres")
        .with_db_name("jobqueue");

    let container = docker.run(postgres_image);

    // Wait a moment for PostgreSQL to start
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let host = container.get_host_ip_address().to_string();
    let port = container.get_host_port_ipv4(5432);

    let config = DatabaseConfig {
        host,
        port: port as u16,
        database: "jobqueue".to_string(),
        user: "postgres".to_string(),
        password: Some("postgres".to_string()),
        max_connections: Some(10),
    };

    let db =
        Arc::new(Database::new(config).map_err(|e| format!("Failed to create database: {}", e))?);

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

    Ok((db, container))
}

/// Setup test database using environment variables
/// Useful for CI/CD or when you want to use an existing database
///
/// WARNING: This requires a real database to be available.
/// For most tests, use setup_mock_database() instead.
#[cfg(feature = "integration-tests")]
pub async fn setup_test_database_from_env() -> Result<Arc<Database>, String> {
    let config = DatabaseConfig::default();
    let db =
        Arc::new(Database::new(config).map_err(|e| format!("Failed to create database: {}", e))?);

    if !db.health_check().await {
        return Err("Database health check failed. Ensure test database is running.".to_string());
    }

    Ok(db)
}

/// Run SQL migrations from a directory
/// This is a simple migration runner - in production you'd use Flyway
///
/// Only available when integration-tests feature is enabled
#[cfg(feature = "integration-tests")]
pub async fn run_sql_migrations(db: &Arc<Database>, migrations: &[&str]) -> Result<(), String> {
    for migration in migrations {
        db.execute_sql(migration)
            .await
            .map_err(|e| format!("Failed to execute migration: {}", e))?;
    }
    Ok(())
}
