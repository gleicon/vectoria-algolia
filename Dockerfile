# ── Stage 1: Build React demo ─────────────────────────────────────────────────
FROM node:22-alpine AS demo-builder

WORKDIR /demo
COPY demo/package.json demo/package-lock.json* ./
RUN npm install 2>/dev/null || npm install --legacy-peer-deps

COPY demo/ ./
RUN npm run build

# ── Stage 2: Build Rust binary ────────────────────────────────────────────────
FROM rust:1.95-slim AS rust-builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev ca-certificates g++ \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Do not copy Cargo.lock — the local one encodes a path dep for vectoria-core
# and would conflict after the sed patch below.
COPY Cargo.toml ./

# Patch vectoria-core to use crates.io version for Docker builds.
ARG VECTORIA_CORE_VERSION=0.1.7
RUN sed -i "s|vectoria-core = { path = \"../vectoria/vectoria-core\" }|vectoria-core = \"${VECTORIA_CORE_VERSION}\"|" Cargo.toml

# Warm dep cache with a stub binary so the real build only recompiles our code.
RUN mkdir -p src && echo 'fn main(){}' > src/main.rs && echo '' > src/lib.rs
RUN cargo fetch

# Patch edgestore's fdp_backend.rs — all 1.0.x versions have a Linux-only bug:
#   `if let Ok(_fd) = as_raw_fd(...)` where as_raw_fd() returns i32, not Result.
# Rust 1.88 made this a hard E0308 error. The file is #[cfg(target_os = "linux")]
# so it never compiled on macOS. Fix: replace the if-let with a plain let,
# keeping the braces intact so the block structure is preserved.
RUN find /usr/local/cargo/registry/src -name "fdp_backend.rs" -path "*/edgestore-*" -print0 | \
    xargs -0 -I{} sed -i \
      -e 's/if let Ok(_fd) = std::os::fd::AsRawFd::as_raw_fd(/let _fd = std::os::fd::AsRawFd::as_raw_fd(/' \
      -e 's/^            ) {$/            ); if true {/' \
      {}

RUN cargo build --release 2>/dev/null; true
# Remove fingerprints for our own crates so the real source triggers a recompile.
RUN find target/release/.fingerprint -maxdepth 1 -name "vectoria*" -exec rm -rf {} + 2>/dev/null; true
RUN rm src/main.rs src/lib.rs

# Build the real binary
COPY src/ ./src/
RUN cargo build --release

# ── Stage 3: Final image ──────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 curl jq \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=rust-builder /build/target/release/vectoria-algolia ./vectoria-algolia
COPY --from=demo-builder /demo/dist ./static
COPY scripts/ ./scripts/

ENV HOST=0.0.0.0
ENV PORT=8108
ENV VECTORIA_INDEX=products
ENV STATIC_DIR=/app/static
ENV FASTEMBED_CACHE_PATH=/data/fastembed

VOLUME ["/data"]
EXPOSE 8108

ENTRYPOINT ["./vectoria-algolia"]
