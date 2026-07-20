# Real-source compatibility results

Test date: 2026-07-20

The checks below use native services on the same Docker network as LAP and call
the same backend routes used by the import dialog.

| Source | Native source check | LAP discovery | Result |
| --- | --- | --- | --- |
| OpenCode 1.18.3 | `GET /global/health` and authenticated `GET /agent` return 200 with native agents. | `POST /api/agents/import/opencode/discover` returns 502 because upstream returns 401. | Incompatible authentication and discovery assumptions. Native OpenCode uses Basic Auth and `/agent`; LAP sends `x-api-key` to `/v1/agents`. Authenticated `/v1/agents` returns the OpenCode HTML application, not an agent JSON collection. |
| Generic OpenAPI 3.1 | `POST /api/v1/runs` returns a completed native run. | Discovery returns 200 and preserves the complete generated OpenAPI document. | Discovery is compatible. Preview correctly reports `openapi_runtime_mapping_required` because the independent service has no `x-lap-runtime`. |
| LangGraph 1.2.9 / Agent Server 0.11.1 | `POST /assistants/search` returns the system-created assistant; `POST /runs/wait` executes the graph and returns its standard `messages` state. | Discovery returns 200 with the native assistant identity. | Discovery is compatible. Preview correctly reports `langgraph_input_mapping_required`; the native output is a message list rather than LAP's default `/output` string. |
| CrewAI OSS 1.15.4 | Native `/api/v1/agents` returns the real configured agent and a local-model kickoff exercises CrewAI's Agent, Task, and sequential Crew orchestration. | Discovery returns 502 because the native service returns 404 for `/inputs`. | The adapter is specifically shaped for CrewAI AMP, not a general self-hosted CrewAI service. A gateway-backed kickoff also exposes a separate runtime mismatch: CrewAI's OpenAI provider calls `chat/completions`, while this LAP deployment returns 405 for that route. |

OpenAPI preview result:

```text
severity: approval_required
code: openapi_runtime_mapping_required
field: source.raw.x-lap-runtime
```

LangGraph preview result:

```text
severity: approval_required
code: langgraph_input_mapping_required
field: source.raw.x-lap-runtime
```
