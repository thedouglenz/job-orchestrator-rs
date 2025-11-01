use async_trait::async_trait;
use executor_core::database::{ImageRegistryRepository, ImageInfo};
use std::collections::HashMap;
use std::sync::Arc;

use crate::connection::Database;

pub struct PostgresImageRegistryRepository {
    db: Arc<Database>,
}

impl PostgresImageRegistryRepository {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl ImageRegistryRepository for PostgresImageRegistryRepository {
    async fn get_image(&self, image_id: &str) -> Result<ImageInfo, String> {
        let query = "SELECT * FROM image_registry WHERE id = $1";
        
        let client = self.db.pool.get().await
            .map_err(|e| format!("Failed to get database client: {}", e))?;

        let rows = client
            .query(query, &[&image_id])
            .await
            .map_err(|e| format!("Failed to get image: {}", e))?;

        if rows.is_empty() {
            return Err(format!("Image not found: {}", image_id));
        }

        let row = &rows[0];
        
        // Parse JSONB columns
        let environment_variables: HashMap<String, String> = row
            .get::<_, Option<String>>("environment_variables")
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        let default_resources: Option<types::image::ResourceRequirements> = row
            .get::<_, Option<String>>("default_resources")
            .and_then(|s| serde_json::from_str(&s).ok())
            .and_then(|v| serde_json::from_value(v).ok());

        let labels: HashMap<String, String> = row
            .get::<_, Option<String>>("labels")
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        Ok(ImageInfo {
            id: row.get("id"),
            full_image_path: row.get("full_image_path"),
            is_active: row.get("is_active"),
            environment_variables,
            default_resources,
            labels,
        })
    }
}
