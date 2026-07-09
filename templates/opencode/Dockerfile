# node:20 (full Debian) already bundles git, curl, ca-certificates, python3,
# make and g++ — everything opencode and better-sqlite3 need. s3fs (+ its
# fuse dependency) is the one extra package needed to mount per-session
# workspace buckets as a session's opencode project directory.
FROM node:20
RUN apt-get update \
    && apt-get install -y --no-install-recommends s3fs \
    && rm -rf /var/lib/apt/lists/*

# Install the opencode CLI via the official installer (detects arch + libc;
# works on both arm64 and amd64, unlike the npm package's optionalDeps).
RUN curl -fsSL https://opencode.ai/install | bash
ENV PATH="/root/.opencode/bin:$PATH"

WORKDIR /app

COPY package.json ./
RUN npm install --omit=dev

COPY src ./src

# /data/opencode-sessions holds each workspace-backed session's isolated
# opencode HOME (session/message history — see session-pool.mjs) and FUSE
# mountpoint; it's under /data so it survives container restarts the same
# way agents.db already does.
RUN mkdir -p /data /data/opencode-sessions /tmp/opencode-workspace

ENV PORT=8080 OPENCODE_PORT=4096 WORKDIR=/tmp/opencode-workspace DB_PATH=/data/agents.db \
    SESSIONS_ROOT=/data/opencode-sessions

EXPOSE 8080

CMD ["node", "src/index.mjs"]
