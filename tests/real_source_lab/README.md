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

See [`RESULTS.md`](./RESULTS.md) for the observed compatibility results.

Use Docker service names in the import dialog. Loopback addresses are only for
host-side inspection and are rejected by LAP's connector SSRF validation.

| Native source | Import dialog endpoint | API key field | Expected first result |
| --- | --- | --- | --- |
| OpenCode  | `http://opencode-native:4096` | `native-opencode` | Discovery should expose any mismatch between native `/agent` + Basic Auth and LAP's assumed `/v1/agents` + `x-api-key`. |
| LangGraph Agent Server | `http://langgraph-native:8123` | empty | Native assistant discovery should work; preview may require mapping because the graph uses the standard `messages` state. |
| Generic OpenAPI agent | `http://openapi-native:8080` | empty | OpenAPI discovery should work; preview should request an execution mapping because the spec contains no LAP-only extension. |
| Self-hosted CrewAI OSS | `http://crewai-native:8080` | empty | AMP-specific discovery is expected to fail because a normal CrewAI service has no universal `/inputs` endpoint. |

Host inspection:

```bash
curl -u opencode:native-opencode http://127.0.0.1:14096/agent
curl http://127.0.0.1:18123/openapi.json
curl http://127.0.0.1:18080/openapi.json
curl http://127.0.0.1:18081/openapi.json
```

## Sources that cannot honestly be represented by a small local HTTP fixture

- **ACP**: the stable protocol transport is a subprocess over stdio. Remote
  HTTP/WebSocket transport remains a draft. A container exposing `GET /agents`
  would test LAP's assumption, not ACP.
- **OpenAI Assistants**: this is an OpenAI-hosted API. Test it against
  `https://api.openai.com` with a real project key; a local clone is not an
  authenticity test.
- **Dify**: use the official Dify Community Edition compose stack, create and
  publish an app in its UI, then use that app's `/v1` endpoint and generated
  API key. A single fake `/info` service is not representative.
- **Elastic Agent Builder**: use Elasticsearch and Kibana 9.2+ with an
  Enterprise trial/license, create an Agent Builder agent and API key, then
  point LAP at the Kibana base URL. Agent Builder is not provided by a small
  standalone agent image.

These boundaries are part of the test result: cloud-only products, licensed
platform features, and stdio protocols should not be silently replaced by
adapter-shaped mocks.
