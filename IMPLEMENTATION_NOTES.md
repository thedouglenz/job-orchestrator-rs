# Implementation Notes

## Database Implementation

The TypeScript implementation uses PostgreSQL with the `pg` library. The Rust implementation should follow the same patterns:

### Key Patterns from TypeScript:

1. **Connection Pooling**: Uses `pg.Pool` (Rust equivalent: `deadpool-postgres`)
2. **Parameterized Queries**: Uses `$1, $2, ...` placeholders (same in Rust `tokio-postgres`)
3. **Raw SQL**: Direct SQL queries, not ORM
4. **Row Mapping**: Each repository has a `mapRow()` method to convert DB rows to domain objects

### Database Schema

The schema is defined in migration files:
- `V1__create_image_registry.sql` - Image registry table
- `V2__create_job_queue.sql` - Job queue table  
- `V3__create_job_execution_history.sql` - Execution history table
- `V5__add_job_io_columns.sql` - Inputs/outputs as JSONB columns
- `V6__create_dag_tables.sql` - DAG execution tables

### Implementation Status

? **Partially Complete**:
- Database connection struct with pooling
- JobQueueRepository skeleton

? **Remaining**:
- Complete JobQueueRepository implementation (needs proper row mapping)
- ImageRegistryRepository  
- JobExecutionHistoryRepository
- DAG repositories (if needed)

### Row Mapping

The TypeScript `mapRow()` methods handle:
- Snake_case to camelCase field conversion
- JSON parsing for JSONB columns (environment_variables, resource_overrides, etc.)
- Date/timestamp parsing
- Optional field handling

Example from TypeScript:
```typescript
private mapRow(row: any): JobStatus {
  return {
    ...row,
    imageId: row.image_id,  // snake_case -> camelCase
    status: row.status as JobStatusEnum,
    createdAt: new Date(row.created_at),
    environmentVariables: JSON.parse(row.environment_variables || '{}'),
    // ...
  };
}
```

In Rust, you'll need to:
1. Extract fields by name from `tokio_postgres::Row`
2. Parse JSONB columns with `serde_json`
3. Handle nullable fields with `Option<T>`
4. Convert timestamps from `chrono::NaiveDateTime` to `chrono::DateTime<Utc>`

### Column Names

The database uses snake_case:
- `image_id`, `job_name`, `k8s_job_name`
- `environment_variables` (JSONB)
- `resource_overrides` (JSONB)
- `dag_execution_id`, `dag_node_execution_id`
- `created_at`, `updated_at`, `claimed_at`, `started_at`, `completed_at`

Rust types use camelCase, so mapping is required.
