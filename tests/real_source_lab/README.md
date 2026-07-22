# Real source lab for “导入智能体”

This lab runs upstream-native services and deliberately does not add routes,
headers, response wrappers, or metadata solely to satisfy LAP import adapters.
An import failure is therefore a useful compatibility result rather than a
fixture failure.

## Start

The main LAP compose stack must already be running because the lab joins its
Docker network:

```bash
docker compose up -d
docker compose -f tests/real_source_lab/compose.yaml up -d --build
docker compose -f tests/real_source_lab/compose.yaml ps
```

Run all native and LAP-side probes after the services are ready:

```bash
bash tests/real_source_lab/probe.sh
```

Run the full import → mapping confirmation → governance → activation →
structured Run round trip for the executable native sources:

```bash
bash tests/real_source_lab/run-e2e.sh
```

See [`RESULTS.md`](./RESULTS.md) for the observed compatibility results.

Use Docker service names in the import dialog. Loopback addresses are only for
host-side inspection and are rejected by LAP's connector SSRF validation.

| Native source | Import dialog endpoint | API key field | Expected first result |
| --- | --- | --- | --- |
| OpenCode  | `http://opencode-native:4096` | `native-opencode` (or `username:password`) | Native Basic Auth + `/agent` discovery works; hidden internal agents are omitted. The legacy wrapper `/v1/agents` contract remains a fallback. |
| LangGraph Agent Server | `http://langgraph-native:8123` | empty | Discovery works. Confirm `input_field=messages` and `output_path=/messages` before execution. |
| Generic OpenAPI agent | `http://openapi-native:8080` | empty | Discovery works. Confirm `/api/v1/runs`, `input_field=messages`, and `output_field=messages` before execution. |
| Self-hosted CrewAI OSS | `http://crewai-native:8080` | empty | LAP returns an actionable 400 directing the operator to OpenAPI / REST. CrewAI AMP remains on its native `/inputs` + async kickoff contract. |
| Dify Community Edition | `http://dify-native/v1` | app key from `dify/bootstrap.sh` | Discovery works once an app is published. Started separately — see [`dify/README.md`](./dify/README.md). |

Host inspection:

```bash
curl -u opencode:native-opencode http://127.0.0.1:14096/agent
curl http://127.0.0.1:18123/openapi.json
curl http://127.0.0.1:18080/openapi.json
curl http://127.0.0.1:18081/openapi.json
```

## Dify

Dify is real but is not part of `compose.yaml`: it is a multi-container product
(api, worker, web, Postgres, Redis, vector store, sandbox, nginx) rather than a
single native service, and its `/v1` API only exists once an app is published
inside a workspace. It therefore has its own fetch/bootstrap flow:

```bash
bash tests/real_source_lab/dify/fetch.sh
cd tests/real_source_lab/dify/upstream
EXPOSE_NGINX_PORT=8088 EXPOSE_NGINX_SSL_PORT=8443 \
  docker compose -f docker-compose.yaml -f ../lap-network.yaml up -d
cd - && bash tests/real_source_lab/dify/bootstrap.sh
```

Details, including why no fake `/info` fixture is provided and what execution
additionally requires, are in [`dify/README.md`](./dify/README.md).

## Sources that cannot honestly be represented by a small local HTTP fixture

- **ACP**: the stable protocol transport is a subprocess over stdio. Remote
  HTTP/WebSocket transport remains a draft. A container exposing `GET /agents`
  would test LAP's assumption, not ACP.
- **OpenAI Assistants**: this is an OpenAI-hosted API. Test it against
  `https://api.openai.com` with a real project key; a local clone is not an
  authenticity test.
- **Elastic Agent Builder**: use Elasticsearch and Kibana 9.2+ with an
  Enterprise trial/license, create an Agent Builder agent and API key, then
  point LAP at the Kibana base URL. Agent Builder is not provided by a small
  standalone agent image.

These boundaries are part of the test result: cloud-only products, licensed
platform features, and stdio protocols should not be silently replaced by
adapter-shaped mocks.
