# syntax=docker/dockerfile:1.7

# Node 24 (LTS): Node 26 segfaults during the webpack build's worker-pool
# teardown on this image (pages generate fine, then the process crashes with
# SIGSEGV / exit 139). Node 24 builds cleanly.
FROM node:24-bookworm-slim AS ui-builder
ARG NPM_REGISTRY=https://registry.npmjs.org
WORKDIR /build/src/ui
COPY src/ui/package.json src/ui/package-lock.json ./
RUN --mount=type=cache,target=/root/.npm \
    npm ci --no-audit --no-fund --registry "$NPM_REGISTRY" \
      --maxsockets 4 \
      --fetch-timeout 120000 \
      --fetch-retries 6 \
      --fetch-retry-mintimeout 20000 \
      --fetch-retry-maxtimeout 120000
COPY src/ui/ ./
RUN npm run build

# aws-sdk-s3's dependency tree requires rustc >= 1.94.
FROM rust:1.94-bookworm AS rust-builder
WORKDIR /build
COPY Cargo.toml Cargo.lock build.rs ./
COPY src ./src
COPY skills ./skills
# Cache mounts persist cargo's registry/git checkouts and the incremental
# build cache across image builds (they're excluded from the resulting layer,
# so `target/` never ends up in the image) - without this, every source edit
# forced a from-scratch `cargo build --release` of the whole dependency tree.
# The binary is copied out to a plain (non-cache-mounted) path so the later
# COPY --from=rust-builder in the runtime stage can see it.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/build/target \
    cargo build --release --bin lite && \
    cp target/release/lite /build/lite-bin

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=rust-builder /build/lite-bin /usr/local/bin/lite
COPY --from=ui-builder /build/src/ui/out /app/ui
COPY config.yaml.example /app/config.yaml.example
COPY deploy/render.config.yaml /app/deploy.config.yaml

ENV HOST=0.0.0.0
ENV PORT=4000
ENV LITELLM_CONFIG=/app/deploy.config.yaml
ENV LITELLM_UI_DIR=/app/ui

EXPOSE 4000
CMD ["lite", "serve"]
