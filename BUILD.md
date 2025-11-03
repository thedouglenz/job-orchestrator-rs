# Building and Pushing Docker Images

This document describes how to build and push the Rust executor Docker image to GCP Artifact Registry.

## Prerequisites

1. **Google Cloud SDK** installed and configured:
   ```bash
   gcloud auth login
   gcloud config set project YOUR_PROJECT_ID
   ```

2. **Docker** installed and running

3. **Artifact Registry repository** created:
   ```bash
   gcloud artifacts repositories create job-orchestrator \
     --repository-format=docker \
     --location=us \
     --description="Job orchestrator Docker images"
   ```

## Quick Start

### Using Make

```bash
# Build and push (uses GCP_PROJECT_ID from gcloud config)
make docker-push

# Build and push with specific tag
make docker-push IMAGE_TAG=v0.1.0

# Build and push release version (from Cargo.toml)
make release

# For local minikube development
make dev  # Builds and loads into minikube
```

### Manual Build and Push

```bash
# Set your GCP project (or export GCP_PROJECT_ID)
export GCP_PROJECT_ID=your-project-id

# Build the image
docker build -t us-docker.pkg.dev/$GCP_PROJECT_ID/job-orchestrator/rust-executor:latest -f Dockerfile .

# Authenticate Docker to Artifact Registry
gcloud auth configure-docker us-docker.pkg.dev

# Push the image
docker push us-docker.pkg.dev/$GCP_PROJECT_ID/job-orchestrator/rust-executor:latest
```

## Image Naming Convention

Images follow this pattern:
```
us-docker.pkg.dev/{GCP_PROJECT_ID}/job-orchestrator/rust-executor:{TAG}
```

**Tags:**
- `latest` - Latest build from main branch
- `{version}` - Semantic version from Cargo.toml (e.g., `0.1.0`)
- `{version}-{branch}-{sha}` - Feature branch builds (e.g., `0.1.0-feat-parallel-abc1234`)

## Local Development with Minikube

For local testing without pushing to GCP:

```bash
# Build the image locally
make docker-build

# Load into minikube
make minikube-load

# Or use the Makefile shortcut
make dev
```

Then update your Helm values to use:
```yaml
executor:
  image:
    repository: rust-executor
    tag: latest
    pullPolicy: Never  # Use local image
```

## CI/CD (GitHub Actions)

The `.github/workflows/rust-executor.yml` workflow automatically:

1. **Tests** on every push/PR:
   - Runs `cargo fmt --check`
   - Runs `cargo clippy`
   - Runs `cargo test`
   - Builds the release binary

2. **Builds and pushes** on pushes to branches:
   - Main branch: Tags with version + `latest`
   - Feature branches: Tags with `version-branch-sha`
   - Uses semantic version from `Cargo.toml`

### Required GitHub Secrets

- `GCP_PROJECT_ID` - Your GCP project ID
- `GCP_SA_KEY` - Service account JSON key with Artifact Registry permissions

## Troubleshooting

### Authentication Errors

```bash
# Re-authenticate
gcloud auth login
gcloud auth configure-docker us-docker.pkg.dev
```

### Permission Errors

Ensure your GCP user/service account has:
- `Artifact Registry Writer` role
- Or `roles/artifactregistry.writer` permission

```bash
gcloud artifacts repositories add-iam-policy-binding job-orchestrator \
  --location=us \
  --member=user:your-email@example.com \
  --role=roles/artifactregistry.writer
```

### Build Errors

If Rust dependencies fail:
```bash
# Update Rust toolchain
rustup update stable

# Clean and rebuild
cargo clean
cargo build --release
```

### Minikube Image Issues

If minikube can't find the image:
```bash
# Use minikube's Docker daemon
eval $(minikube docker-env)
docker build -t rust-executor:latest -f Dockerfile .
eval $(minikube docker-env -u)
```

## Updating the Image in Helm

After pushing a new image, update your Helm deployment:

```bash
helm upgrade rust-executor ./helm/rust-executor \
  --namespace job-orchestrator \
  --set executor.image.tag=v0.1.0 \
  --reuse-values
```

Or update `values.yaml`:
```yaml
executor:
  image:
    repository: us-docker.pkg.dev/YOUR_PROJECT/job-orchestrator/rust-executor
    tag: "v0.1.0"
```
