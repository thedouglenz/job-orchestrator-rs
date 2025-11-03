# Migration Runner for Tests

The integration tests automatically run all database migrations from:
```
/Users/doug/dev/job-sys/packages/database/migrations/sql/
```

## How It Works

The `run_migrations()` function in `integration_test.rs`:
1. Loads all 8 migration files using `include_str!`
2. Executes them in order (V1 through V8)
3. Uses the `Database::execute_sql()` method to run each migration

## Migration Files Included

- V1: `create_image_registry.sql` - Image registry table
- V2: `create_job_queue.sql` - Job queue table (partitioned)
- V3: `create_job_execution_history.sql` - Execution history
- V4: `create_dead_letter_queue.sql` - Dead letter queue
- V5: `add_job_io_columns.sql` - Inputs/outputs support
- V6: `create_dag_tables.sql` - DAG execution tables
- V7: `add_job_archival.sql` - Archival support
- V8: `add_sibling_parallelization_fields.sql` - Parallel execution

## Benefits

✅ **Automatic**: Migrations run automatically when tests start
✅ **Fresh Schema**: Each test gets a clean database with latest schema
✅ **No Manual Setup**: No need to run Flyway manually before tests
✅ **CI/CD Ready**: Works the same locally and in CI/CD

## Usage

```rust
#[tokio::test]
async fn test_example() {
    let db = setup_test_database().await.expect("Failed to setup test database");
    // Database is fully migrated and ready to use
    // Container automatically destroyed when test completes
}
```

## Alternative: Manual Migration

If you prefer to run migrations manually (e.g., using Flyway):

```bash
cd /Users/doug/dev/job-sys/packages/database
flyway migrate \
  -url=jdbc:postgresql://localhost:5432/jobqueue \
  -user=postgres \
  -password=postgres \
  -locations=filesystem:migrations/sql
```

Then use `setup_test_database_from_env()` instead.

