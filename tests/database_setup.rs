// Database setup utilities for integration tests

use database::{Database, DatabaseConfig};
use std::sync::Arc;

/// Setup test database using testcontainers
/// Returns both the database connection and the container (which keeps it alive)
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
pub async fn run_sql_migrations(db: &Arc<Database>, migrations: &[&str]) -> Result<(), String> {
    // Note: This requires exposing the pool or adding a method to Database
    // For now, this is a placeholder showing the structure

    // In practice, you would:
    // 1. Read SQL files from a migrations directory
    // 2. Execute them in order
    // 3. Track which migrations have been run

    // For integration tests, you can either:
    // - Pre-run migrations using Flyway
    // - Copy migration files to tests/fixtures/ and execute them here
    // - Use a simple SQL migration runner

    Ok(())
}
