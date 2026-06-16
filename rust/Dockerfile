# syntax=docker/dockerfile:1.23
#
# Stage 1: Chef - prepare recipe (dependency manifest)
# musl-tools provides the C cross-compiler for x86_64-unknown-linux-musl.
# Install the musl Rust target in the cached chef layer so planner/builder
# stages do not block on rustup network traffic during every build.
# Clang + mold linker speed up the final link step 2-5x; they cost ~30s to
# install but pay back on every rebuild.
FROM rust:1.96.0-bookworm AS chef
RUN apt-get update && apt-get install -y musl-tools protobuf-compiler clang mold && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef cargo-auditable --locked
RUN rustup set profile minimal && rustup target add x86_64-unknown-linux-musl
WORKDIR /app
ENV RUSTFLAGS="-C link-arg=-fuse-ld=mold" \
    RUSTUP_SKIP_UPDATE_CHECK=1

# Stage 2: Planner - generate recipe.json
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Stage 3: Builder - musl static build
FROM chef AS builder

# Build dependencies first (cached if Cargo.toml/lock unchanged).
# BuildKit cache mounts keep the cargo registry and the `target/` compilation
# artefacts across builds, so a rebuild with unchanged source becomes seconds
# instead of minutes. The mounts are scoped per-Dockerfile by the `id` label
# to avoid polluting the sibling images (sidecar, orchestrator).
COPY --from=planner /app/recipe.json recipe.json
# Vendored sqlx fork referenced via [patch.crates-io] in Cargo.toml —
# cargo-chef cook needs the patch manifests present at cook time, else:
# "failed to read /app/third_party/sqlx-nomysql/Cargo.toml".
COPY third_party third_party
RUN --mount=type=cache,id=agentforge-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=agentforge-cargo-git,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=agentforge-server-target,target=/app/target \
    cargo chef cook --release --target x86_64-unknown-linux-musl --recipe-path recipe.json

# Build application. The `cp` after build lifts the binary out of the cached
# `target/` before the mount is dropped so the next stage can COPY it.
COPY . .
RUN --mount=type=cache,id=agentforge-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=agentforge-cargo-git,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=agentforge-server-target,target=/app/target \
    cargo auditable build --release --target x86_64-unknown-linux-musl --bin agentforge-server \
    && cp /app/target/x86_64-unknown-linux-musl/release/agentforge-server /usr/local/bin/agentforge-server

# Stage 4: Minimal alpine runtime (~8MB base, static binary = no libc required)
FROM alpine:3.23 AS runtime
RUN apk --no-cache add ca-certificates curl git

# Create non-root user
RUN addgroup -S agentforge && adduser -S agentforge -G agentforge

COPY --from=builder /usr/local/bin/agentforge-server /usr/local/bin/agentforge-server

# Include migrations for runtime migration execution. Migrations live outside
# `target/` so they are still available even when target is a cache mount.
COPY --from=builder /app/crates/db/migrations /opt/agentforge/migrations

USER agentforge
EXPOSE 4003

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -sf http://localhost:4003/health || exit 1

ENTRYPOINT ["/usr/local/bin/agentforge-server"]
