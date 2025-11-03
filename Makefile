.PHONY: help build push test docker-build docker-push minikube-load

# Configuration
GCP_PROJECT_ID ?= $(shell gcloud config get-value project 2>/dev/null)
REGISTRY := us-docker.pkg.dev
REPOSITORY := job-orchestrator
IMAGE_NAME := rust-executor
IMAGE_TAG ?= latest
FULL_IMAGE := $(REGISTRY)/$(GCP_PROJECT_ID)/$(REPOSITORY)/$(IMAGE_NAME):$(IMAGE_TAG)

help: ## Show this help message
	@echo 'Usage: make [target]'
	@echo ''
	@echo 'Available targets:'
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  %-15s %s\n", $$1, $$2}'

build: ## Build the Rust binary
	cargo build --release --bin job-orchestrator

test: ## Run tests
	cargo test

docker-build: ## Build Docker image
	@if [ -z "$(GCP_PROJECT_ID)" ]; then \
		echo "Error: GCP_PROJECT_ID not set. Set it or run: gcloud config set project YOUR_PROJECT"; \
		exit 1; \
	fi
	docker build -t $(FULL_IMAGE) -f Dockerfile .

docker-push: docker-build ## Build and push Docker image to GCP Artifact Registry
	@if [ -z "$(GCP_PROJECT_ID)" ]; then \
		echo "Error: GCP_PROJECT_ID not set. Set it or run: gcloud config set project YOUR_PROJECT"; \
		exit 1; \
	fi
	@echo "Authenticating Docker to GCP Artifact Registry..."
	gcloud auth configure-docker $(REGISTRY)
	@echo "Pushing $(FULL_IMAGE)..."
	docker push $(FULL_IMAGE)
	@echo "Pushed: $(FULL_IMAGE)"

minikube-load: docker-build ## Build image and load into minikube
	@echo "Loading $(FULL_IMAGE) into minikube..."
	minikube image load $(FULL_IMAGE)
	@echo "Image loaded. You can now use: rust-executor:latest in your Helm values"

# Build and push with version tag
release: ## Build and push with version from Cargo.toml
	@VERSION=$$(grep '^version' Cargo.toml | head -1 | awk -F'"' '{print $$2}'); \
	echo "Building and pushing version $$VERSION..."; \
	$(MAKE) docker-push IMAGE_TAG=$$VERSION; \
	$(MAKE) docker-push IMAGE_TAG=latest

# Quick local development: build, load to minikube
dev: minikube-load ## Build and load to minikube for local development
