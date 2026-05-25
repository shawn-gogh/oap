#!/usr/bin/env bash
# Start the local PostgreSQL dev cluster (idempotent).
#
# Used two ways:
#   1. As the E2B sandbox start command (e2b.toml `start_cmd`) so the DB is up
#      the moment a sandbox boots — no agent action required.
#   2. By dev-up.sh, for humans in an interactive shell.
#
# No sudo needed — `user` owns the cluster at /home/user/pgdata.
set -euo pipefail

PG_VERSION=$(ls /usr/lib/postgresql 2>/dev/null | sort -V | tail -1)
PG_BIN="/usr/lib/postgresql/${PG_VERSION}/bin"
PG_DATA="/home/user/pgdata"

if "${PG_BIN}/pg_ctl" -D "${PG_DATA}" status >/dev/null 2>&1; then
  echo "[start-db] PostgreSQL already running."
  exit 0
fi

echo "[start-db] Starting PostgreSQL ${PG_VERSION}..."
"${PG_BIN}/pg_ctl" -D "${PG_DATA}" start -w -t 30 -l /tmp/postgres.log \
  || { echo "[start-db] ERROR: pg_ctl failed — postgres log:" >&2; cat /tmp/postgres.log >&2 || true; exit 1; }
echo "[start-db] PostgreSQL started."
