# Quick Start Guide

## Prerequisites

- Kubernetes cluster with kubectl configured
- Helm 3.0+ installed
- Docker image built and pushed to your registry

## Quick Installation

1. **Update Helm dependencies** (downloads PostgreSQL chart):
   ```bash
   cd helm/rust-executor
   helm dependency update
   ```

2. **Install with default values**:
   ```bash
   helm install rust-executor . \
     --namespace job-orchestrator \
     --create-namespace
   ```

3. **Install with custom image**:
   ```bash
   helm install rust-executor . \
     --namespace job-orchestrator \
     --create-namespace \
     --set executor.image.repository=your-registry/rust-executor \
     --set executor.image.tag=v0.1.0 \
     --set postgresql.auth.password=secure-password
   ```

4. **Install with custom values file**:
   ```bash
   cp values.example.yaml my-values.yaml
   # Edit my-values.yaml
   helm install rust-executor . \
     --namespace job-orchestrator \
     --create-namespace \
     -f my-values.yaml
   ```

5. **Install for Minikube**:
   ```bash
   # Create secrets file from example
   cp secrets-minikube.yaml.example secrets-minikube.yaml
   # Edit secrets-minikube.yaml with your GCP project and password
   
   helm install rust-executor . \
     --namespace job-orchestrator \
     --create-namespace \
     -f values-minikube.yaml \
     -f secrets-minikube.yaml
   
   # Ensure gcr-secret exists for image pulling:
   kubectl create secret docker-registry gcr-secret \
     --docker-server=https://us-docker.pkg.dev \
     --docker-username=oauth2accesstoken \
     --docker-password="$(gcloud auth print-access-token)" \
     --docker-email=your-email@example.com \
     --namespace job-orchestrator
   ```

## Verify Installation

```bash
# Check deployment status
kubectl get deployments -n job-orchestrator

# Check pods
kubectl get pods -n job-orchestrator

# View logs
kubectl logs -f deployment/rust-executor-executor -n job-orchestrator

# Check ConfigMap
kubectl get configmap rust-executor -n job-orchestrator -o yaml
```

## Database Migration

Before the executor can work, you need to apply the database schema:

1. The database schema is in `~/dev/job-sys/packages/database/migrations/`
2. Apply migrations using Flyway or your preferred migration tool
3. Ensure migration V8 is applied for parallel sibling execution features

Example with Flyway:
```bash
# From ~/dev/job-sys/packages/database
flyway migrate \
  -url=jdbc:postgresql://rust-executor-postgresql:5432/jobqueue \
  -user=jobqueue \
  -password=YOUR_PASSWORD
```

## Upgrade

```bash
helm upgrade rust-executor . \
  --namespace job-orchestrator \
  -f my-values.yaml
```

## Uninstall

```bash
helm uninstall rust-executor --namespace job-orchestrator
```

**Note:** This does NOT delete PostgreSQL data if persistence is enabled.
