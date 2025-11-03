# Test Database Setup Guide

## Recommended Approach: testcontainers-rs

**testcontainers-rs** is the recommended approach because:
- ✅ **Automated**: No manual setup required
- ✅ **Isolated**: Each test gets a fresh database
- ✅ **CI/CD Ready**: Works in automated environments
- ✅ **No Cleanup**: Containers automatically destroyed after tests
- ✅ **Already a dependency**: We have `testcontainers = "0.15"` in Cargo.toml

### Prerequisites

1. **Docker installed and running**
   ```bash
   docker --version
   docker ps  # Should work without errors
   ```

2. **Enable testcontainers feature** (when implemented)

### How It Works

testcontainers automatically:
1. Spins up a PostgreSQL container when tests start
2. Exposes a random port on localhost
3. Destroys the container when tests finish
4. Each test gets a fresh database (no state leakage)

### Usage

```rust
#[tokio::test]
async fn test_example() {
    let (db, _container) = setup_test_database_with_container().await
        .expect("Failed to setup test database");
    
    // Container stays alive for the duration of the test
    // Database is automatically cleaned up when container is dropped
    
    // Your test code here...
}
```

---

## Alternative 1: Local Docker PostgreSQL (Manual)

Good for: Quick local testing without Docker automation

### Setup

```bash
# Start PostgreSQL container
docker run -d \
  --name job-orchestrator-test-db \
  -e POSTGRES_USER=jobqueue \
  -e POSTGRES_PASSWORD=password \
  -e POSTGRES_DB=jobqueue \
  -p 5433:5432 \
  postgres:15

# Run migrations (from job-sys directory)
cd ../job-sys/packages/database
flyway migrate \
  -url=jdbc:postgresql://localhost:5433/jobqueue \
  -user=jobqueue \
  -password=password \
  -locations=filesystem:migrations/sql
```

### Environment Variables

```bash
export DATABASE_HOST=localhost
export DATABASE_PORT=5433
export DATABASE_NAME=jobqueue
export DATABASE_USER=jobqueue
export DB_PASSWORD=password
```

### Run Tests

```bash
cargo test --test integration_test
```

### Cleanup

```bash
docker stop job-orchestrator-test-db
docker rm job-orchestrator-test-db
```

---

## Alternative 2: Docker Compose

Good for: Consistent setup across team members

### Create `docker-compose.test.yml`

```yaml
version: '3.8'
services:
  postgres:
    image: postgres:15
    environment:
      POSTGRES_USER: jobqueue
      POSTGRES_PASSWORD: password
      POSTGRES_DB: jobqueue
    ports:
      - "5433:5432"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U jobqueue"]
      interval: 5s
      timeout: 5s
      retries: 5
```

### Usage

```bash
# Start database
docker-compose -f docker-compose.test.yml up -d

# Run migrations
cd ../job-sys/packages/database
flyway migrate \
  -url=jdbc:postgresql://localhost:5433/jobqueue \
  -user=jobqueue \
  -password=password \
  -locations=filesystem:migrations/sql

# Run tests
export DATABASE_HOST=localhost DATABASE_PORT=5433 DATABASE_NAME=jobqueue DATABASE_USER=jobqueue DB_PASSWORD=password
cargo test --test integration_test

# Stop database
docker-compose -f docker-compose.test.yml down
```

---

## Migration Files Location

The SQL migration files are located at:
```
/Users/doug/dev/job-sys/packages/database/migrations/sql/
```

Migration files:
- `V1__create_image_registry.sql`
- `V2__create_job_queue.sql`
- `V3__create_job_execution_history.sql`
- `V4__create_dead_letter_queue.sql`
- `V5__add_job_io_columns.sql`
- `V6__create_dag_tables.sql`
- `V7__add_job_archival.sql`
- `V8__add_sibling_parallelization_fields.sql`

---

## Recommendation

**Use testcontainers-rs** because:
1. It's already set up in the codebase
2. Zero manual configuration
3. Works the same locally and in CI/CD
4. Each test is isolated (no shared state)

The only requirement is Docker running, which is standard for development.

