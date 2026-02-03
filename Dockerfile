# Clawbernetes Multi-Stage Dockerfile
# Builds: claw-gateway, clawnode, clawbernetes (CLI)

# ============================================
# Stage 1: Build
# ============================================
FROM rust:1.83-bookworm AS builder

WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests first for better caching
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Build release binaries
RUN cargo build --release \
    -p claw-gateway-server \
    -p clawnode \
    -p claw-cli

# ============================================
# Stage 2: Gateway Runtime
# ============================================
FROM debian:bookworm-slim AS gateway

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/claw-gateway /usr/local/bin/

EXPOSE 8080

ENTRYPOINT ["claw-gateway"]
CMD ["0.0.0.0:8080"]

# ============================================
# Stage 3: Node Runtime
# ============================================
FROM debian:bookworm-slim AS node

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/clawnode /usr/local/bin/

ENTRYPOINT ["clawnode"]

# ============================================
# Stage 4: CLI Runtime
# ============================================
FROM debian:bookworm-slim AS cli

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/clawbernetes /usr/local/bin/

ENTRYPOINT ["clawbernetes"]

# ============================================
# Stage 5: All-in-One (default)
# ============================================
FROM debian:bookworm-slim AS full

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/claw-gateway /usr/local/bin/
COPY --from=builder /build/target/release/clawnode /usr/local/bin/
COPY --from=builder /build/target/release/clawbernetes /usr/local/bin/

# Default to showing help
CMD ["clawbernetes", "--help"]
