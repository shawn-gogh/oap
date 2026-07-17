FROM python:3.12-slim-bookworm

# System deps: node for the opencode runtime, git/curl/ripgrep for real agent work.
RUN apt-get update && apt-get install -y --no-install-recommends \
        curl ca-certificates git ripgrep unzip \
    && curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*

# The opencode runtime (real coding agent CLI).
ARG OPENCODE_VERSION=1.18.3
RUN npm install -g opencode-ai@${OPENCODE_VERSION} && opencode --version

# A2A protocol server.
RUN pip install --no-cache-dir "a2a-sdk[http-server]==0.3.26" uvicorn

WORKDIR /app
COPY server.py entrypoint.sh ./
RUN chmod +x entrypoint.sh

# Non-root, with writable workspaces for opencode sessions.
RUN useradd -m agent && mkdir -p /workspaces && chown -R agent:agent /workspaces /app
USER agent
ENV HOME=/home/agent

EXPOSE 9200
ENV A2A_PORT=9200
ENTRYPOINT ["/app/entrypoint.sh"]
