# Real-source compatibility results

Test date: 2026-07-21

The checks below use native services on the same Docker network as LAP and call
the same backend routes used by the import dialog.

| Source | Native source check | LAP discovery | Result |
| --- | --- | --- | --- |
| OpenCode 1.18.3 | `GET /global/health` and authenticated `GET /agent` return 200 with native agents. | Discovery returns 200 using Basic Auth and `/agent`; hidden internal agents are filtered. | Compatible. Native name-only identities and `prompt` are normalized without changing the provider-neutral import shape. `/v1/agents` + `x-api-key` remains a tested fallback for the LAP wrapper. |
| Generic OpenAPI 3.1 | `POST /api/v1/runs` returns a completed native run. | Discovery returns 200 and preserves the complete generated OpenAPI document. | Compatible after explicit mapping. A real structured Run with `messages[]` input completed and persisted the native `messages[]` response. |
| LangGraph 1.2.9 / Agent Server 0.11.1 | `POST /assistants/search` returns the system-created assistant; the Agent Server exposes thread-scoped streaming runs. | Discovery returns 200 with the native assistant identity. | Compatible after explicit State mapping. LAP uses `threads/{thread_id}/runs/stream` and projects messages, nodes, subgraphs, interrupts, resumable cursors, and final state into Run. |
| CrewAI OSS 1.15.4 | Native `/api/v1/agents` returns the configured agent and a local-model kickoff exercises CrewAI's orchestration. | CrewAI AMP discovery detects the service's OpenAPI 3 document and returns an actionable HTTP 400. | Deliberately routed to OpenAPI / REST because CrewAI OSS defines no universal remote discovery/execution HTTP protocol. The CrewAI provider remains compatible with AMP's `/inputs`, `/kickoff`, and `/status/{id}` contract. |

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

Real Run result:

```text
openapi: PASS (messages[] -> messages[])
langgraph: PASS (messages[] -> /messages)
```
