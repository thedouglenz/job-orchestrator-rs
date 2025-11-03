# Rust Executor Helm Chart

This Helm chart deploys the Rust-based Kubernetes Job Executor.

## Prerequisites

- Kubernetes 1.19+
- Helm 3.0+
- PostgreSQL database (included via Bitnami chart or external)

## Installation

### Basic Installation

```bash
# Add the chart (if using a repository)
helm repo add rust-executor ./helm/rust-executor

# Install with default values
helm install my-executor ./helm/rust-executor

# Or with custom values
helm install my-executor ./helm/rust-executor -f my-values.yaml
```

### Using External Database

```yaml
# my-values.yaml
postgresql:
  enabled: false

externalDatabase:
  host: postgres.example.com
  port: 5432
  database: jobqueue
  username: jobqueue
  existingSecret: postgres-secret
  existingSecretPasswordKey: password
```

## Configuration

### Executor Configuration

| Parameter | Description | Default |
|-----------|-------------|---------|
| `executor.enabled` | Enable executor deployment | `true` |
| `executor.replicaCount` | Number of executor replicas | `2` |
| `executor.image.repository` | Container image repository | `us-docker.pkg.dev/YOUR_PROJECT/job-orchestrator/rust-executor` |
| `executor.image.tag` | Container image tag | Chart `appVersion` |
| `executor.topics` | Comma-separated topics or "ALL" | `"default"` |
| `executor.batchSize` | Maximum jobs to claim per batch | `5` |
| `executor.pollInterval` | Polling interval in milliseconds (polling mode only) | `5000` |
| `executor.eventDriven` | Enable event-driven mode (recommended) | `true` |
| `executor.kubernetesNamespace` | Namespace where jobs will be created | `"default"` |

### Database Configuration

The chart includes Bitnami PostgreSQL as a dependency, or you can use an external database.

| Parameter | Description | Default |
|-----------|-------------|---------|
| `postgresql.enabled` | Deploy PostgreSQL with chart | `true` |
| `postgresql.auth.username` | Database username | `jobqueue` |
| `postgresql.auth.password` | Database password | `changeme` (?? CHANGE IN PRODUCTION) |
| `postgresql.auth.database` | Database name | `jobqueue` |

### Resource Limits

Default resource requests/limits:

```yaml
executor:
  resources:
    requests:
      cpu: 200m
      memory: 512Mi
    limits:
      cpu: 1000m
      memory: 1Gi
```

## Features

### Event-Driven Mode (Recommended)

The executor supports event-driven execution mode for improved performance:

```yaml
executor:
  eventDriven: true  # Enabled by default
```

This eliminates polling latency and enables parallel sibling execution in DAGs.

### Polling Mode (Legacy)

For backward compatibility, polling mode is available:

```yaml
executor:
  eventDriven: false
  pollInterval: 5000  # milliseconds
```

## RBAC

The chart creates a Role and RoleBinding with permissions to:
- Create, read, update, delete Kubernetes Jobs
- Read Pod logs and status
- List and watch Jobs and Pods

## Database Migration

**Note:** The database schema must be created separately. The executor expects:
- PostgreSQL database with the schema from `~/dev/job-sys/packages/database/migrations/`
- Migration V8 must be applied for parallel sibling execution features

You can use Flyway or another migration tool to apply the schema.

## Uninstallation

```bash
helm uninstall my-executor
```

**Warning:** This will delete the deployment but not the PostgreSQL data (if using included PostgreSQL with persistence enabled).

## Examples

### Development Environment

```yaml
executor:
  replicaCount: 1
  eventDriven: true
  topics: "ALL"

postgresql:
  enabled: true
  auth:
    password: dev-password
  primary:
    persistence:
      enabled: false  # Ephemeral storage for dev
```

### Production Environment

```yaml
executor:
  replicaCount: 3
  eventDriven: true
  topics: "production,high-priority"
  resources:
    requests:
      cpu: 500m
      memory: 1Gi
    limits:
      cpu: 2000m
      memory: 2Gi

postgresql:
  enabled: false  # Use managed database

externalDatabase:
  host: managed-postgres.example.com
  port: 5432
  database: jobqueue_prod
  username: jobqueue
  existingSecret: postgres-prod-secret
```

## Troubleshooting

### Executor not starting

1. Check database connectivity:
   ```bash
   kubectl logs deployment/<release-name>-executor
   ```

2. Verify database credentials in secret:
   ```bash
   kubectl get secret <release-name>-postgresql -o jsonpath='{.data.password}' | base64 -d
   ```

3. Check RBAC permissions:
   ```bash
   kubectl describe role <release-name>
   ```

### Jobs not being executed

1. Verify topics match:
   ```bash
   kubectl get configmap <release-name> -o yaml | grep EXECUTOR_TOPICS
   ```

2. Check if event-driven mode is enabled:
   ```bash
   kubectl get configmap <release-name> -o yaml | grep EXECUTOR_EVENT_DRIVEN
   ```

3. Verify executor can create jobs:
   ```bash
   kubectl auth can-i create jobs --namespace default --as=system:serviceaccount:default:<release-name>
   ```

## Support

For issues and questions, please refer to the main project repository.
