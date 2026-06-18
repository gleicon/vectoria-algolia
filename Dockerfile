# ── Stage 1: Build React demo ─────────────────────────────────────────────────
FROM node:22-alpine AS demo-builder

WORKDIR /demo
COPY demo/package.json demo/package-lock.json* ./
RUN npm install 2>/dev/null || npm install --legacy-peer-deps

COPY demo/ ./
RUN npm run build

# ── Stage 2: Build Rust binary ────────────────────────────────────────────────
FROM rust:1.88-slim AS rust-builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy Cargo manifests
COPY Cargo.toml Cargo.lock ./

# Patch vectoria-core to use crates.io version for Docker builds.
# For local dev builds the path dep is used; in Docker we override it.
ARG VECTORIA_CORE_VERSION=0.1.6
RUN sed -i "s|vectoria-core = { path = \"../vectoria/vectoria-core\" }|vectoria-core = \"${VECTORIA_CORE_VERSION}\"|" Cargo.toml

# Warm the dependency cache
RUN mkdir -p src && echo 'fn main(){}' > src/main.rs && echo '' > src/lib.rs
RUN cargo build --release 2>/dev/null; true
RUN rm src/main.rs src/lib.rs

# Build the real binary
COPY src/ ./src/
RUN cargo build --release

# ── Stage 3: Final image ──────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=rust-builder /build/target/release/vectoria-algolia ./vectoria-algolia
COPY --from=demo-builder /demo/dist ./static
COPY scripts/products.json ./scripts/products.json
COPY scripts/load_products.sh ./scripts/load_products.sh

ENV HOST=0.0.0.0
ENV PORT=8108
ENV VECTORIA_INDEX=products
ENV STATIC_DIR=/app/static
ENV FASTEMBED_CACHE_PATH=/data/fastembed

VOLUME ["/data"]
EXPOSE 8108

ENTRYPOINT ["./vectoria-algolia"]
