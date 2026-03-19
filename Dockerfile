# MFN System Production Container
# Multi-stage build: Rust workspace + Go layer3 + Zig layer1 -> single runtime

# =============================================================================
# Stage 1: Rust Builder — mfn-gateway, layer2, layer4, layer5
# =============================================================================
FROM rust:1.75-slim AS rust-builder
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY Cargo.toml Cargo.lock* ./
COPY mfn-core/ mfn-core/
COPY mfn-integration/ mfn-integration/
COPY mfn-binary-protocol/ mfn-binary-protocol/
COPY layer2-rust-dsr/ layer2-rust-dsr/
COPY layer4-rust-cpe/ layer4-rust-cpe/
COPY layer5-rust-psr/ layer5-rust-psr/
COPY src/ src/

RUN cargo build --release \
    --bin mfn-gateway \
    --bin layer2_socket_server \
    --bin layer4_socket_server \
    --bin layer5_socket_server

# =============================================================================
# Stage 2: Go Builder — layer3 ALM
# =============================================================================
FROM golang:1.21-alpine AS go-builder
RUN apk add --no-cache gcc musl-dev

WORKDIR /build/layer3
COPY layer3-go-alm/ .
RUN go mod download && go build -o layer3_alm .

# =============================================================================
# Stage 3: Zig Builder — layer1 IFR
# =============================================================================
FROM debian:bookworm-slim AS zig-builder
RUN apt-get update && apt-get install -y \
    curl \
    xz-utils \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

RUN curl -L https://ziglang.org/download/0.14.0/zig-linux-x86_64-0.14.0.tar.xz | tar -xJ && \
    mv zig-linux-x86_64-0.14.0 /opt/zig && \
    ln -s /opt/zig/zig /usr/local/bin/zig

WORKDIR /build/layer1
COPY layer1-zig-ifr/ .
RUN zig build -Doptimize=ReleaseFast

# =============================================================================
# Stage 4: Runtime — all binaries + configs in one image
# =============================================================================
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    supervisor \
    curl \
    procps \
    && rm -rf /var/lib/apt/lists/*

# Create directories
RUN mkdir -p /app/bin /app/config /data/mfn/memory /tmp

# Copy Rust binaries
COPY --from=rust-builder /build/target/release/mfn-gateway /app/bin/
COPY --from=rust-builder /build/target/release/layer2_socket_server /app/bin/
COPY --from=rust-builder /build/target/release/layer4_socket_server /app/bin/
COPY --from=rust-builder /build/target/release/layer5_socket_server /app/bin/

# Copy Go binary
COPY --from=go-builder /build/layer3/layer3_alm /app/bin/

# Copy Zig binary
COPY --from=zig-builder /build/layer1/zig-out/bin/ifr_socket_server /app/bin/

# Make binaries executable
RUN chmod +x /app/bin/*

# Copy supervisor config
COPY docker/config/supervisord.conf /etc/supervisord.conf

# Copy application config
COPY docker/config/mfn_config.json /app/config/

# Environment
ENV MFN_DATA_DIR=/data/mfn/memory \
    MFN_API_PORT=8080

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=30s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

EXPOSE 8080

VOLUME ["/data/mfn/memory"]

CMD ["/usr/bin/supervisord", "-n", "-c", "/etc/supervisord.conf"]
