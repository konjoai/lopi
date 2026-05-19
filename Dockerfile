# Multi-stage build for lopi.
#
# Stage 1 — build: compiles the workspace in release mode.
# Stage 2 — runtime: minimal Debian image with just the binary.
#
# Build:  docker build -t lopi .
# Run:    docker run -p 3000:3000 -p 3002:3002 -e ANTHROPIC_API_KEY=... lopi sail

# ─── Stage 1: build ────────────────────────────────────────────────────────────
FROM rust:1.87-slim-bookworm AS builder

WORKDIR /build

# Install system deps needed for compilation.
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    git \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace manifests first so dependency layer is cached independently
# of source changes.
COPY Cargo.toml Cargo.lock ./
COPY crates/lopi-core/Cargo.toml        crates/lopi-core/Cargo.toml
COPY crates/lopi-context/Cargo.toml     crates/lopi-context/Cargo.toml
COPY crates/lopi-git/Cargo.toml         crates/lopi-git/Cargo.toml
COPY crates/lopi-agent/Cargo.toml       crates/lopi-agent/Cargo.toml
COPY crates/lopi-memory/Cargo.toml      crates/lopi-memory/Cargo.toml
COPY crates/lopi-orchestrator/Cargo.toml crates/lopi-orchestrator/Cargo.toml
COPY crates/lopi-ratelimit/Cargo.toml   crates/lopi-ratelimit/Cargo.toml
COPY crates/lopi-ui/Cargo.toml          crates/lopi-ui/Cargo.toml
COPY crates/lopi-remote/Cargo.toml      crates/lopi-remote/Cargo.toml
COPY crates/lopi-webhook/Cargo.toml     crates/lopi-webhook/Cargo.toml
COPY crates/lopi-toon/Cargo.toml        crates/lopi-toon/Cargo.toml
COPY crates/lopi-spec/Cargo.toml        crates/lopi-spec/Cargo.toml
COPY crates/lopi-github/Cargo.toml      crates/lopi-github/Cargo.toml
COPY crates/lopi-tools/Cargo.toml       crates/lopi-tools/Cargo.toml
COPY crates/lopi-app/Cargo.toml         crates/lopi-app/Cargo.toml

# Stub out every lib.rs and main.rs so `cargo fetch` + dependency compile
# succeeds without the real source (speeds up layer caching).
RUN find crates -name Cargo.toml | while read f; do \
      dir=$(dirname "$f"); \
      mkdir -p "$dir/src"; \
      echo "fn main() {}" > "$dir/src/main.rs" 2>/dev/null || true; \
      echo "" > "$dir/src/lib.rs" 2>/dev/null || true; \
    done && \
    mkdir -p src && echo "fn main() {}" > src/main.rs

RUN cargo build --release --bin lopi 2>/dev/null || true

# Now copy the full source and rebuild only what changed.
COPY . .

# The SvelteKit Forge dist must exist (even as an empty dir) so rust-embed
# compiles. In production builds, run `npm run build` in web/ first and
# COPY web/dist into the image; for server-only deploys the placeholder
# is served instead.
RUN mkdir -p web/dist

RUN cargo build --release --bin lopi

# ─── Stage 2: runtime ──────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    git \
    && rm -rf /var/lib/apt/lists/*

# Non-root user for security.
RUN useradd -m -u 1000 lopi
USER lopi
WORKDIR /home/lopi

COPY --from=builder /build/target/release/lopi /usr/local/bin/lopi

# Persistent data volume — SQLite database and customer stores live here.
VOLUME ["/home/lopi/.lopi"]

# lopi sail (Forge dashboard + agent API)
EXPOSE 3000
# lopi serve-app (GitHub App OAuth + Stripe webhooks)
EXPOSE 3002

ENV RUST_LOG=lopi=info,tower_http=warn

# Default: start the Forge dashboard.
# Override with `docker run lopi serve-app` for the SaaS app server.
CMD ["lopi", "sail", "--port", "3000"]
