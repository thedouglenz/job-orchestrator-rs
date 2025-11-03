# Integration Tests

This directory contains integration tests for the Rust job executor.

## Test Structure

- `common/` - Shared test utilities and fixtures
- `integration_test.rs` - End-to-end integration tests

## Running Tests

### Unit Tests (No database required)
```bash
cargo test --package executor-core
cargo test --package dag-orchestrator
cargo test --package k8s-client
```

### Integration Tests

Integration tests require a PostgreSQL database. You have two options:

#### Option 1: Use Test Database via Environment Variables
```bash
export DATABASE_HOST=localhost
export DATABASE_PORT=5432
export DATABASE_NAME=jobqueue_test
export DATABASE_USER=jobqueue
export DB_PASSWORD=password

cargo test --test integration_test
```

#### Option 2: Use Testcontainers (when feature enabled)
```bash
cargo test --features integration-tests --test integration_test
```

Note: The `integration-tests` feature requires Docker to be running.

## Test Coverage

### Unit Tests
- ✅ Executor core logic (retry, error handling, topic filtering)
- ✅ DAG orchestrator (input resolution, execution modes)
- ✅ K8s client (error handling, serialization)
- ✅ Database repositories (type conversions, JSONB handling)

### Integration Tests (Structure ready, requires database)
- ⏳ Full job lifecycle (submit → claim → execute → complete)
- ⏳ Job retry flow with failing jobs
- ⏳ Topic filtering (specific topics vs "ALL")
- ⏳ Event-driven vs polling mode comparison

### Future Tests
- Compatibility tests (Rust vs TypeScript executor)
- Performance benchmarks
- Error scenario tests
- Cluster integration tests

