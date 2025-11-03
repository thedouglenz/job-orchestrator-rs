# Build stage - use latest Rust (needed for edition2024)
FROM rust:latest AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace files
COPY Cargo.toml Cargo.lock ./

# Copy all workspace members
COPY k8s-client ./k8s-client
COPY types ./types
COPY dag-orchestrator ./dag-orchestrator
COPY executor-core ./executor-core
COPY database ./database
COPY src ./src

# Build the binary
RUN cargo build --release --bin job-orchestrator

# Runtime stage - use Debian Trixie (matches Rust builder GLIBC)
FROM debian:trixie-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Copy the binary from builder
COPY --from=builder /app/target/release/job-orchestrator /app/job-orchestrator

# Run as root for now (can fix user creation later if needed)
# USER executor

CMD ["/app/job-orchestrator"]
