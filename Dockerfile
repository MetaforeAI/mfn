# MFN System Production Container
# Multi-stage build for optimized production deployment

# Stage 1: Zig Builder
FROM debian:bookworm-slim AS zig-builder
RUN apt-get update && apt-get install -y \
    curl \
    xz-utils \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

# Install Zig
RUN curl -L https://ziglang.org/download/0.11.0/zig-linux-x86_64-0.11.0.tar.xz | tar -xJ && \
    mv zig-linux-x86_64-0.11.0 /opt/zig && \
    ln -s /opt/zig/zig /usr/local/bin/zig

# Copy and build Layer 1 IFR
WORKDIR /build/layer1
COPY layer1-zig-ifr/ .
RUN zig build-exe src/main.zig -O ReleaseFast && \
    zig build-exe src/socket_server.zig -O ReleaseFast

# Stage 2: Rust Builder
FROM rust:1.75-slim AS rust-builder
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Build Layer 2 DSR
WORKDIR /build/layer2
COPY layer2-rust-dsr/ .
RUN cargo build --release --bin layer2_socket_server

# Build Layer 4 CPE
WORKDIR /build/layer4
COPY layer4-rust-cpe/ .
RUN cargo build --release --bin layer4_socket_server

# Build MFN Core
WORKDIR /build/mfn-core
COPY mfn-core/ .
RUN cargo build --release

# Stage 3: Go Builder
FROM golang:1.21-alpine AS go-builder
RUN apk add --no-cache gcc musl-dev

# Build Layer 3 ALM
WORKDIR /build/layer3
COPY layer3-go-alm/ .
COPY go.mod go.sum* ./
RUN go mod download || true
RUN go build -o layer3_server cmd/server/main.go || \
    go build -o layer3_server main.go || \
    echo "Layer 3 build pending"

# Build High Performance Protocol Stack
WORKDIR /build
COPY high_performance_protocol_stack.go .
COPY mfn_protocol_integration.go .
RUN go build -o hp_protocol_stack high_performance_protocol_stack.go

# Stage 4: Production Runtime
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    python3 \
    python3-pip \
    python3-venv \
    supervisor \
    sqlite3 \
    curl \
    netcat-openbsd \
    procps \
    htop \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN groupadd -r mfn && useradd -r -g mfn -d /app -s /bin/bash mfn

# Create application directories
RUN mkdir -p \
    /app/bin \
    /app/lib \
    /app/config \
    /app/data \
    /app/logs \
    /app/sockets \
    /app/backups \
    /app/dashboard \
    /app/scripts \
    && chown -R mfn:mfn /app

# Set up Python virtual environment
USER mfn
WORKDIR /app
RUN python3 -m venv /app/venv
ENV PATH="/app/venv/bin:$PATH"

# Copy Python requirements and install
COPY --chown=mfn:mfn requirements.txt .
RUN pip install --no-cache-dir --upgrade pip && \
    pip install --no-cache-dir \
    numpy \
    scipy \
    scikit-learn \
    fastapi \
    uvicorn \
    websockets \
    prometheus-client \
    psutil \
    aiofiles \
    pydantic \
    sqlalchemy \
    alembic

# Copy built binaries from builder stages
COPY --from=zig-builder --chown=mfn:mfn /build/layer1/main /app/bin/layer1_ifr
COPY --from=zig-builder --chown=mfn:mfn /build/layer1/socket_server /app/bin/layer1_socket_server
COPY --from=rust-builder --chown=mfn:mfn /build/layer2/target/release/layer2_socket_server /app/bin/layer2_socket_server
COPY --from=rust-builder --chown=mfn:mfn /build/layer4/target/release/layer4_socket_server /app/bin/layer4_socket_server
COPY --from=go-builder --chown=mfn:mfn /build/layer3/layer3_server /app/bin/layer3_server 2>/dev/null || echo "Layer 3 not available"
COPY --from=go-builder --chown=mfn:mfn /build/hp_protocol_stack /app/bin/hp_protocol_stack

# Copy Python scripts
COPY --chown=mfn:mfn unified_socket_client.py /app/lib/
COPY --chown=mfn:mfn optimized_mfn_client.py /app/lib/
COPY --chown=mfn:mfn add_persistence.py /app/lib/
COPY --chown=mfn:mfn mfn_client.py /app/lib/

# Copy configuration and scripts
COPY --chown=mfn:mfn docker/config/ /app/config/
COPY --chown=mfn:mfn docker/scripts/ /app/scripts/

# Make scripts executable
USER root
RUN chmod +x /app/scripts/*.sh /app/bin/*
USER mfn

# Environment variables
ENV MFN_DATA_DIR=/app/data \
    MFN_LOG_DIR=/app/logs \
    MFN_SOCKET_DIR=/app/sockets \
    MFN_BACKUP_DIR=/app/backups \
    MFN_CONFIG_DIR=/app/config \
    PYTHONPATH=/app/lib:$PYTHONPATH \
    MFN_ENV=production

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=60s --retries=3 \
    CMD /app/scripts/health_check.sh || exit 1

# Expose ports
EXPOSE 8080 8081 8082 9090 3000

# Volume mounts for persistence
VOLUME ["/app/data", "/app/logs", "/app/backups"]

# Start supervisor
USER root
CMD ["/usr/bin/supervisord", "-c", "/app/config/supervisord.conf"]