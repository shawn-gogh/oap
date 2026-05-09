#!/usr/bin/env bash
# One-shot bootstrap: Postgres + schema + AWS infra + dev + worker.
# Re-runnable. Picks up where it left off.
set -euo pipefail

# 1. Make sure .env exists. First-time users get prompted to fill it in.
if [ ! -f .env ]; then
  cp .env.example .env
  cat <<'EOF'

Created .env from .env.example. Open it and fill in:
  • MASTER_KEY                              (any string >= 8 chars)
  • AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY
  • LITELLM_API_BASE, LITELLM_API_KEY

Then re-run: npm run quickstart

EOF
  exit 0
fi

# 2. Boot the local Postgres container and wait for it.
echo "→ Postgres"
docker compose up -d
until docker compose exec -T postgres pg_isready -U litellm -d litellm_agents >/dev/null 2>&1; do
  sleep 1
done

# 3. Push the Prisma schema.
echo "→ schema"
npx prisma db push --accept-data-loss --skip-generate >/dev/null
npx prisma generate >/dev/null

# 4. Provision AWS (ECR, IAM, SG, cluster, task def). setup.sh writes the
#    four output values straight into .env, so re-running quickstart is safe.
echo "→ AWS"
./setup.sh >/tmp/setup.log 2>&1 || { tail -40 /tmp/setup.log; exit 1; }
tail -8 /tmp/setup.log

# 5. Start Next.js + the reconciler worker side-by-side.
echo "→ web + worker on http://localhost:3000"
exec npx concurrently -n web,worker -c blue,magenta \
  "next dev" \
  "tsx --env-file=.env src/worker/index.ts"
