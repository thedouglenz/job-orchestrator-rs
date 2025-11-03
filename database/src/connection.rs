use deadpool_postgres::{Config, Pool, Runtime};
use tokio_postgres::NoTls;
use tracing::{error, info};

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub password: Option<String>,
    pub max_connections: Option<usize>,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            host: std::env::var("DATABASE_HOST")
                .unwrap_or_else(|_| "localhost".to_string()),
            port: std::env::var("DATABASE_PORT")
                .unwrap_or_else(|_| "5432".to_string())
                .parse()
                .unwrap_or(5432),
            database: std::env::var("DATABASE_NAME")
                .unwrap_or_else(|_| "jobqueue".to_string()),
            user: std::env::var("DATABASE_USER")
                .unwrap_or_else(|_| "jobqueue".to_string()),
            password: std::env::var("DB_PASSWORD").ok(),
            max_connections: Some(20),
        }
    }
}

pub struct Database {
    pub(crate) pool: Pool,
}

impl Database {
    pub fn new(config: DatabaseConfig) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Build tokio_postgres config directly
        let mut pg_config = tokio_postgres::Config::new();
        pg_config.host(&config.host);
        pg_config.port(config.port);
        pg_config.dbname(&config.database);
        pg_config.user(&config.user);
        if let Some(password) = &config.password {
            pg_config.password(password);
        }

        // Create deadpool config with individual fields
        let mut pool_config = deadpool_postgres::Config::new();
        pool_config.host = Some(config.host);
        pool_config.port = Some(config.port);
        pool_config.user = Some(config.user);
        pool_config.password = config.password;
        pool_config.dbname = Some(config.database);
        pool_config.pool = Some(deadpool_postgres::PoolConfig {
            max_size: config.max_connections.unwrap_or(20),
            ..Default::default()
        });

        let pool = pool_config
            .create_pool(Some(Runtime::Tokio1), NoTls)
            .map_err(|e| format!("Failed to create connection pool: {}", e))?;

        Ok(Self { pool })
    }

    pub async fn health_check(&self) -> bool {
        match self.pool.get().await {
            Ok(client) => {
                match client.query_one("SELECT 1", &[]).await {
                    Ok(_) => true,
                    Err(e) => {
                        error!(error = ?e, "Database health check query failed");
                        false
                    }
                }
            }
            Err(e) => {
                error!(error = ?e, "Database health check failed - couldn't get client");
                false
            }
        }
    }

    pub async fn close(&self) {
        info!("Closing database connection pool");
        // Pool will close when dropped
    }

    /// Execute raw SQL (useful for migrations)
    pub async fn execute_sql(&self, sql: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let client = self.pool.get().await
            .map_err(|e| format!("Failed to get database client: {}", e))?;
        
        client.batch_execute(sql).await
            .map_err(|e| format!("Failed to execute SQL: {}", e))?;
        
        Ok(())
    }
}
