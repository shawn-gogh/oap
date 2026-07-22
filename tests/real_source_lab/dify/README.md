# Real Dify Community Edition for the source lab

The other services in this lab are single containers running real upstream
software. Dify cannot be reduced that way: it is a multi-container product
whose `/v1` service API only exists once an app has been created and published
inside a workspace, against a real Postgres, Redis, and vector store. A
hand-written service answering `GET /info` and `POST /chat-messages` would pass
LAP's adapter by construction — it would be derived from
`sdk/providers/dify_import_agents.rs` and `sessions::external_bridge::invoke_dify`
rather than tested against them — so it would confirm nothing.

So this directory runs the genuine Community Edition stack instead, and keeps
the LAP-specific parts down to one compose overlay.

LAP already has a fast, asserted regression test for its own Dify code path in
`tests/managed_agents_support/flows/dify_governance.rs` (wiremock, runs in CI).
Use this lab when the question is "does LAP work against real Dify"; use that
test when the question is "did we break LAP's Dify handling".

## Layout

| File | Role |
| --- | --- |
| `fetch.sh` | Downloads the pinned official `docker/` directory into `upstream/` (gitignored) |
| `lap-network.yaml` | Compose overlay: joins nginx to LAP's network as `dify-native`, names the project |
| `bootstrap.sh` | Drives the real console API to create an admin, an app, and a service API key |

Nothing from Dify is vendored. Its compose is ~1300 lines and its nginx service
bind-mounts config templates from that same tree, so a trimmed copy would drift
from upstream and stop being the real stack.

## Run

The main LAP stack must already be up, because the overlay joins its network:

```bash
docker compose up -d

bash tests/real_source_lab/dify/fetch.sh
cd tests/real_source_lab/dify/upstream
EXPOSE_NGINX_PORT=8088 EXPOSE_NGINX_SSL_PORT=8443 \
  docker compose -f docker-compose.yaml -f ../lap-network.yaml up -d
```

First boot pulls several GB and takes a few minutes; the api container runs
database migrations before it answers.

Then create the app and key:

```bash
bash tests/real_source_lab/dify/bootstrap.sh
```

It prints the two values the import dialog needs. Use the Docker service name —
`http://dify-native/v1` — because LAP's connector SSRF validation rejects
loopback addresses. `http://127.0.0.1:8088` is for host-side inspection and for
the console UI only.

To tear down (`-v` also drops Dify's databases, so the account and app go too):

```bash
cd tests/real_source_lab/dify/upstream
docker compose -f docker-compose.yaml -f ../lap-network.yaml down
```

## What this does and does not exercise

**Discovery works immediately.** `GET /v1/info` answers as soon as an app has a
key, so import, preview, governance, and drift detection are all testable
straight after `bootstrap.sh`.

**Execution needs one more step.** `invoke_dify` calls `POST /v1/chat-messages`,
which a Dify app cannot answer until the workspace has a model provider. Add one
under **Settings → Model Provider** at http://127.0.0.1:8088. Pointing it at
LAP's own gateway (`http://lap:4000/v1`, OpenAI-compatible) mirrors what
`crewai-native` does and keeps the loop inside the lab.

**App mode matters, and is worth testing both ways.** `bootstrap.sh` creates a
`chat` app by default. LAP uses Dify's streaming Chat API for chat-mode apps.
For workflow mode it discovers the published input form from `/parameters`,
uses `/workflows/run`, projects node events into Run steps and child
invocations, persists file outputs as canonical artifacts, and maps Human
Input pauses into Run continuation requests. Set `DIFY_APP_MODE=workflow` to
exercise that path against a real workflow app.

## Version pin

Pinned to Dify **1.16.0** via `DIFY_VERSION`. `bootstrap.sh` uses console
endpoints (`/console/api/setup`, `/login`, `/apps`, `/apps/{id}/api-keys`) that
the web UI calls but Dify does not document as a stable contract. If a version
bump breaks the script, the same four steps in the console UI are the fallback —
and the break is itself a finding worth recording in `../RESULTS.md`.
