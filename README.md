# Job Orchestrator (Rust)

A Rust rewrite of the job executor from the job-sys project. This implementation uses idiomatic Rust patterns including traits for abstraction, async/await for concurrency, and proper error handling.

## Structure

The project is organized as a Cargo workspace with the following crates:

- **`types`**: Shared type definitions (JobStatus, ExecutorConfig, etc.)
- **`k8s-client`**: Kubernetes client operations (create jobs, get status, get logs)
- **`executor-core`**: Core executor logic with trait-based database abstractions
- **`dag-orchestrator`**: DAG (Directed Acyclic Graph) orchestration logic
- **`job-orchestrator`**: Main binary entry point

## Architecture

The executor follows idiomatic Rust OOP patterns:

- **Traits for abstraction**: Database operations are abstracted through traits (`JobQueueRepository`, `ImageRegistryRepository`, etc.) allowing for different implementations
- **Async/await**: All I/O operations are async using Tokio
- **Error handling**: Uses `Result<T, E>` with custom error types
- **Resource management**: Uses `Arc` for shared ownership and `tokio::sync::RwLock` for synchronization

## Key Components

### JobExecutor

The main executor class that:
- Polls the job queue at configurable intervals
- Claims jobs and executes them in Kubernetes
- Monitors job status and handles completion/failure
- Supports retry logic and error categorization

### K8sClient

Handles all Kubernetes operations:
- Creating Kubernetes Jobs from JobExecutionSpec
- Getting job status
- Retrieving pod logs
- Deleting jobs

### DAGOrchestrator

Manages DAG (workflow) execution:
- Tracks node dependencies
- Creates jobs for child nodes when parents complete
- Handles failure propagation (marks descendants as skipped)

## Usage

### Environment Variables

- `EXECUTOR_TOPICS`: Comma-separated list of topics to process (or "ALL" for all topics)
- `EXECUTOR_POLL_INTERVAL`: Milliseconds between queue polls (default: 5000)
- `EXECUTOR_BATCH_SIZE`: Number of jobs to claim per poll (default: 5)
- `K8S_NAMESPACE`: Kubernetes namespace (default: "default")
- `EXECUTOR_INSTANCE_ID`: Executor instance identifier
- `K8S_IN_CLUSTER`: Set to "true" if running inside Kubernetes
- `OUTPUT_COLLECTOR_IMAGE`: Image for the output collector sidecar
- `API_SERVICE_URL`: URL for the API service

### Database Implementation

The executor uses trait-based abstractions for database operations. You need to implement:

- `JobQueueRepository`: For job queue operations
- `ImageRegistryRepository`: For image registry access
- `JobExecutionHistoryRepository`: For execution history

See `src/executor.rs` for stub implementations that need to be replaced with actual PostgreSQL implementations.

## Building

```bash
cargo build --release
```

## Running

```bash
cargo run --release
```

## Design Decisions

1. **Trait-based design**: Allows for easy testing and different database implementations
2. **Separate crates**: Kubernetes operations are in a separate crate (`k8s-client`) for reusability
3. **Arc wrapping**: Shared resources like repositories and K8sClient are wrapped in `Arc` for shared ownership across async tasks
4. **Async spawning**: Job monitoring runs in separate async tasks to avoid blocking the main polling loop

## Next Steps

1. Implement actual PostgreSQL database repositories
2. Add comprehensive error handling and retry logic
3. Add metrics and observability
4. Add unit and integration tests
5. Create Docker images for deployment
