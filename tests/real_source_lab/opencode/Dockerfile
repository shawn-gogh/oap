FROM node:20-bookworm-slim

ARG OPENCODE_VERSION=1.18.3

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl git \
    && curl -fsSL https://opencode.ai/install | bash -s -- --version "${OPENCODE_VERSION}" \
    && /root/.opencode/bin/opencode --version \
    && rm -rf /var/lib/apt/lists/*

ENV PATH="/root/.opencode/bin:$PATH"

WORKDIR /workspace

COPY opencode.json ./
COPY agents ./.opencode/agent

RUN git init -q \
    && git config user.email source-lab@example.invalid \
    && git config user.name source-lab \
    && git add . \
    && git commit -qm init

EXPOSE 4096
