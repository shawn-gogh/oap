import uuid
import asyncio
import json
import urllib.request
from fastapi import FastAPI, Request, Header
from fastapi.responses import JSONResponse, PlainTextResponse, HTMLResponse, StreamingResponse
from pydantic import BaseModel
from typing import Dict, Any, Optional

app = FastAPI(title="A2A Agent Test Fixture")

# In-memory task store, keyed by task_id. Shared across scenarios for simplicity.
tasks_db: Dict[str, Dict[str, Any]] = {}


class JsonRpcRequest(BaseModel):
    jsonrpc: str = "2.0"
    id: Any = None
    method: str
    params: Dict[str, Any] = {}


# ---------------------------------------------------------------------------
# Scenario catalog. Each scenario is reachable at:
#   GET  /scenarios/{name}/.well-known/agent-card.json   (discovery)
#   POST /scenarios/{name}/rpc                            (execution, where relevant)
#
# Point the import dialog's "服务地址" (endpoint) at:
#   http://<host>:8080                    for the default happy-path agent
#   http://<host>:8080/scenarios/{name}    for a specific edge case
# ---------------------------------------------------------------------------
SCENARIOS = {
    "missing-name": "Agent card omits the required `name` field -> strict discovery rejects the card.",
    "no-identity": "Agent card has no extension `id` and no required legacy `url` -> strict discovery rejects the card.",
    "malformed-json": "Agent card endpoint returns 200 with a body that is not valid JSON -> ImportAgentsError::Decode.",
    "http-500": "Agent card endpoint returns HTTP 500 -> ImportAgentsError::Upstream.",
    "auth-required": "Agent card endpoint requires `Authorization: Bearer <api_key>`; wrong/missing key -> 401 Upstream error.",
    "high-risk": "Agent card carries high-risk raw fields (permissions/network/filesystem/secrets/...) -> normalize_agent() marks the import approval_required.",
    "v1": "Strict A2A 1.0 Agent Card and PascalCase JSON-RPC operations.",
    "stream": "A2A 0.3 message/stream SSE status, message, and artifact events.",
    "v1-stream": "A2A 1.0 SendStreamingMessage SSE status, message, and artifact events.",
    "rich": "A2A 0.3 response containing text, structured data, and an embedded file artifact.",
    "push": "A2A 0.3 task registers and delivers an authenticated push notification.",
    "v1-push": "A2A 1.0 task registers and delivers an authenticated push notification.",
    "v1-complete": "A2A 1.0 complete operation matrix including subscribe, push CRUD, and extended card.",
    "required-extension": "Agent Card requires an unknown protocol extension and must be rejected by strict clients.",
    "task-fail": "message/send returns an async task that resolves to status.state=failed.",
    "task-input-required": "message/send returns an async task that resolves to status.state=input-required (unsupported continuation).",
    "task-timeout": "message/send returns an async task that never leaves status.state=working (exercises the bridge's poll deadline).",
    "rpc-error": "message/send always returns a JSON-RPC `error` object instead of a result.",
    "mutable": "Card content can be changed at runtime via PUT /scenarios/mutable/card, for exercising source drift detection / sync_source.",
}

# Runtime-overridable fields for the `mutable` scenario, mutated via
# PUT /scenarios/mutable/card. Starts identical to the default card content
# (minus id/url, which _agent_card always derives) so a fresh import is in_sync.
mutable_overrides: Dict[str, Any] = {}


def _agent_card(base_url: str, scenario: Optional[str], **overrides: Any) -> Dict[str, Any]:
    prefix = f"/scenarios/{scenario}" if scenario else ""
    if scenario in ("v1", "v1-stream", "v1-push", "v1-complete"):
        card = {
            "id": "example-a2a-agent-v1",
            "name": "A2A 1.0 示例智能体",
            "description": "用于验证严格 A2A 1.0 JSON-RPC 绑定。",
            "supportedInterfaces": [
                {
                    "url": f"{base_url}{prefix}/rpc",
                    "protocolBinding": "JSONRPC",
                    "protocolVersion": "1.0",
                }
            ],
            "version": "1.0.0",
            "capabilities": {
                "streaming": scenario in ("v1-stream", "v1-complete"),
                "pushNotifications": scenario in ("v1-push", "v1-complete"),
                "extendedAgentCard": scenario == "v1-complete",
            },
            "defaultInputModes": ["text/plain"],
            "defaultOutputModes": ["text/plain"],
            "skills": [
                {
                    "id": "example-v1",
                    "name": "A2A 1.0 示例交互",
                    "description": "返回同步结果或创建异步任务。",
                    "tags": ["example", "test", "v1"],
                }
            ],
        }
        card.update(overrides)
        return card
    card = {
        "id": f"example-a2a-agent{('-' + scenario) if scenario else ''}",
        "name": "A2A 示例智能体",
        "description": "一个用于演示 A2A 协议交互的示例智能体，支持同步和异步任务处理。",
        "protocolVersion": "0.3",
        "url": f"{base_url}{prefix}/rpc",
        "preferredTransport": "JSONRPC",
        "version": "1.0.0",
        "capabilities": {
            "streaming": scenario == "stream",
            "pushNotifications": scenario == "push",
        },
        "defaultInputModes": ["text/plain"],
        "defaultOutputModes": ["text/plain"],
        "skills": [
            {
                "id": "example",
                "name": "示例交互",
                "description": "返回同步结果或创建异步任务。",
                "tags": ["example", "test"],
            }
        ],
    }
    if scenario == "required-extension":
        card["capabilities"]["extensions"] = [
            {
                "uri": "https://extensions.example/required/v1",
                "required": True,
            }
        ]
    card.update(overrides)
    return card


@app.get("/", response_class=HTMLResponse)
def index():
    rows = "".join(f"<li><code>{name}</code> — {desc}</li>" for name, desc in SCENARIOS.items())
    return (
        "<h1>A2A test fixture</h1>"
        "<p>Default happy-path agent card is served at <code>/.well-known/agent-card.json</code>.</p>"
        f"<p>Named scenarios (mount at <code>/scenarios/&lt;name&gt;</code>):</p><ul>{rows}</ul>"
    )


@app.get("/healthz", response_class=PlainTextResponse)
def healthz():
    return "ok"


# --- Default happy-path agent -------------------------------------------------

@app.get("/.well-known/agent-card.json")
def get_agent_card(request: Request):
    base_url = str(request.base_url).rstrip("/")
    return _agent_card(base_url, None)


@app.post("/rpc")
async def rpc_endpoint(payload: JsonRpcRequest, a2a_version: Optional[str] = Header(None)):
    return await _handle_rpc(None, payload, a2a_version)


# --- Scenario agents -----------------------------------------------------------

@app.get("/scenarios/{scenario}/.well-known/agent-card.json")
async def scenario_agent_card(
    scenario: str, request: Request, authorization: Optional[str] = Header(None)
):
    base_url = str(request.base_url).rstrip("/")

    if scenario == "missing-name":
        card = _agent_card(base_url, scenario)
        del card["name"]
        return card

    if scenario == "no-identity":
        card = _agent_card(base_url, scenario)
        del card["id"]
        del card["url"]
        return card

    if scenario == "malformed-json":
        return PlainTextResponse(
            content='{"name": "broken", "url": "not json from here on ->', status_code=200
        )

    if scenario == "http-500":
        return JSONResponse(status_code=500, content={"error": "simulated upstream failure"})

    if scenario == "auth-required":
        expected = "Bearer test-secret-key"
        if authorization != expected:
            return JSONResponse(
                status_code=401, content={"error": "missing or invalid bearer token"}
            )
        return _agent_card(base_url, scenario)

    if scenario == "high-risk":
        return _agent_card(
            base_url,
            scenario,
            permissions=["shell.exec", "filesystem.write"],
            network=["*.internal.example.com"],
            filesystem=["/data"],
            secrets=["PROD_DB_PASSWORD"],
            side_effects=["sends outbound email"],
        )

    if scenario == "mutable":
        return _agent_card(base_url, scenario, **mutable_overrides)

    if scenario in (
        "v1",
        "stream",
        "v1-stream",
        "rich",
        "push",
        "v1-push",
        "v1-complete",
        "required-extension",
        "task-fail",
        "task-input-required",
        "task-timeout",
        "rpc-error",
    ):
        return _agent_card(base_url, scenario)

    return JSONResponse(status_code=404, content={"error": f"unknown scenario '{scenario}'"})


@app.put("/scenarios/mutable/card")
async def set_mutable_card(request: Request):
    """Merge fields into the `mutable` scenario's agent card, e.g.
    `curl -X PUT .../scenarios/mutable/card -d '{"description": "changed"}'`
    to simulate the remote agent's metadata drifting after import."""
    body = await request.json()
    if not isinstance(body, dict):
        return JSONResponse(status_code=400, content={"error": "body must be a JSON object"})
    mutable_overrides.update(body)
    return {"overrides": mutable_overrides}


@app.post("/scenarios/mutable/reset")
def reset_mutable_card():
    mutable_overrides.clear()
    return {"overrides": mutable_overrides}


@app.post("/scenarios/{scenario}/rpc")
async def scenario_rpc(
    scenario: str,
    payload: JsonRpcRequest,
    a2a_version: Optional[str] = Header(None),
):
    return await _handle_rpc(scenario, payload, a2a_version)


# --- Shared JSON-RPC handling ----------------------------------------------

async def _handle_rpc(
    scenario: Optional[str],
    payload: JsonRpcRequest,
    a2a_version: Optional[str],
):
    method = payload.method
    params = payload.params
    is_v1 = scenario in ("v1", "v1-stream", "v1-push", "v1-complete")
    expected_version = "1.0" if is_v1 else "0.3"

    if a2a_version != expected_version:
        return {
            "jsonrpc": "2.0",
            "id": payload.id,
            "error": {
                "code": -32009,
                "message": f"expected A2A-Version {expected_version}",
            },
        }

    if method == ("SendStreamingMessage" if is_v1 else "message/stream"):
        if scenario not in ("stream", "v1-stream", "v1-complete"):
            return {
                "jsonrpc": "2.0",
                "id": payload.id,
                "error": {"code": -32004, "message": "streaming is not enabled"},
            }

        async def events():
            task_id = f"task_{uuid.uuid4().hex[:12]}"
            updates = [
                {
                    "taskId": task_id,
                    "contextId": f"ctx_{uuid.uuid4().hex[:8]}",
                    "status": {"state": _task_state("working", is_v1)},
                },
                {
                    "taskId": task_id,
                    "message": {
                        "role": "ROLE_AGENT" if is_v1 else "agent",
                        "messageId": f"msg_{uuid.uuid4().hex[:12]}",
                        "parts": [{"text": "streamed response"}],
                    },
                },
                {
                    "taskId": task_id,
                    "status": {"state": _task_state("completed", is_v1)},
                    "artifact": {
                        "artifactId": "stream-report",
                        "parts": [{"data": {"stream": "complete"}}],
                    },
                },
            ]
            for update in updates:
                result = update if not is_v1 else {"statusUpdate": update}
                envelope = {"jsonrpc": "2.0", "id": payload.id, "result": result}
                yield f"data: {json.dumps(envelope)}\n\n"
                await asyncio.sleep(0.01)

        return StreamingResponse(events(), media_type="text/event-stream")

    if method == ("SendMessage" if is_v1 else "message/send"):
        if scenario == "rpc-error":
            return {
                "jsonrpc": "2.0",
                "id": payload.id,
                "error": {"code": -32000, "message": "simulated agent-side failure"},
            }

        message = params.get("message", {})
        prompt = "".join(
            part.get("text", "")
            for part in message.get("parts", [])
            if part.get("kind") in (None, "text")
        )

        resume_task_id = message.get("taskId") or params.get("taskId")
        if resume_task_id:
            task = tasks_db.get(resume_task_id)
            if not task:
                return {
                    "jsonrpc": "2.0",
                    "id": payload.id,
                    "error": {"code": -32602, "message": "Task not found"},
                }
            task["status"] = {"state": "completed"}
            task["text"] = (
                f"【续接完成】已收到补充输入：'{prompt}'。"
                f"原始问题：'{task['prompt']}'。任务已完成。"
            )
            result = {
                "id": resume_task_id,
                "status": {"state": _task_state("completed", is_v1)},
                "artifacts": [{"parts": [{"text": task["text"]}]}],
            }
            return {
                "jsonrpc": "2.0",
                "id": payload.id,
                "result": {"task": result} if is_v1 else result,
            }

        terminal_scenarios = {
            "task-fail": "failed",
            "task-input-required": "input-required",
            "task-timeout": "working",  # never advances
        }
        if scenario in terminal_scenarios:
            task_id = f"task_{uuid.uuid4().hex[:12]}"
            tasks_db[task_id] = {
                "id": task_id,
                "status": {"state": "working"},
                "prompt": prompt,
                "final_state": terminal_scenarios[scenario],
            }
            asyncio.create_task(_advance_task(task_id, delay=2))
            result = {
                "id": task_id,
                "contextId": f"ctx_{uuid.uuid4().hex[:8]}",
                "status": {"state": _task_state("working", is_v1)},
            }
            return {
                "jsonrpc": "2.0",
                "id": payload.id,
                "result": {"task": result} if is_v1 else result,
            }

        if scenario == "rich":
            return {
                "jsonrpc": "2.0",
                "id": payload.id,
                "result": {
                    "message": {
                        "role": "agent",
                        "messageId": f"msg_{uuid.uuid4().hex[:12]}",
                        "parts": [{"kind": "text", "text": "rich response"}],
                    },
                    "artifacts": [
                        {
                            "artifactId": "analysis",
                            "name": "analysis.json",
                            "parts": [{"kind": "data", "data": {"answer": 42}}],
                        },
                        {
                            "artifactId": "note",
                            "name": "note.txt",
                            "parts": [
                                {
                                    "kind": "file",
                                    "file": {
                                        "name": "note.txt",
                                        "mimeType": "text/plain",
                                        "bytes": "aGVsbG8=",
                                    },
                                }
                            ],
                        },
                    ],
                },
            }

        is_async = "async" in prompt.lower() or scenario in ("push", "v1-push")
        if not is_async:
            reply_text = f"【同步响应】你好！我是 A2A 智能体。我已收到你的请求，你问的是：'{prompt}'"
            result = (
                {
                    "message": {
                        "role": "ROLE_AGENT",
                        "messageId": f"msg_{uuid.uuid4().hex[:12]}",
                        "parts": [{"text": reply_text}],
                    }
                }
                if is_v1
                else {"text": reply_text}
            )
            return {"jsonrpc": "2.0", "id": payload.id, "result": result}

        task_id = f"task_{uuid.uuid4().hex[:12]}"
        tasks_db[task_id] = {
            "id": task_id,
            "status": {"state": "working"},
            "prompt": prompt,
            "final_state": "completed",
            "protocol_version": expected_version,
        }
        asyncio.create_task(_advance_task(task_id, delay=3))
        result = {
            "id": task_id,
            "contextId": f"ctx_{uuid.uuid4().hex[:8]}",
            "status": {"state": _task_state("working", is_v1)},
        }
        return {
            "jsonrpc": "2.0",
            "id": payload.id,
            "result": {"task": result} if is_v1 else result,
        }

    if method == (
        "CreateTaskPushNotificationConfig" if is_v1 else "tasks/pushNotificationConfig/set"
    ):
        task_id = params.get("taskId")
        task_data = tasks_db.get(task_id)
        if not task_data:
            return {
                "jsonrpc": "2.0",
                "id": payload.id,
                "error": {"code": -32001, "message": "Task not found"},
            }
        task_data["push"] = params if is_v1 else params.get("pushNotificationConfig")
        task_data["push"]["id"] = task_data["push"].get("id") or f"push_{uuid.uuid4().hex[:8]}"
        return {
            "jsonrpc": "2.0",
            "id": payload.id,
            "result": (
                task_data["push"]
                if is_v1
                else {"taskId": task_id, "pushNotificationConfig": task_data["push"]}
            ),
        }

    if method == ("SubscribeToTask" if is_v1 else "tasks/resubscribe"):
        task_id = params.get("id")
        task_data = tasks_db.get(task_id)
        if not task_data:
            return {
                "jsonrpc": "2.0",
                "id": payload.id,
                "error": {"code": -32001, "message": "Task not found"},
            }

        async def subscription():
            result = {
                "id": task_id,
                "status": {
                    "state": _task_state(task_data["status"]["state"], is_v1)
                },
            }
            envelope_result = {"task": result} if is_v1 else result
            yield f"data: {json.dumps({'jsonrpc': '2.0', 'id': payload.id, 'result': envelope_result})}\n\n"

        return StreamingResponse(subscription(), media_type="text/event-stream")

    if method == (
        "DeleteTaskPushNotificationConfig"
        if is_v1
        else "tasks/pushNotificationConfig/delete"
    ):
        task_id = params.get("taskId") if is_v1 else params.get("id")
        if task_id in tasks_db:
            tasks_db[task_id].pop("push", None)
        return {"jsonrpc": "2.0", "id": payload.id, "result": {}}

    if method == ("GetExtendedAgentCard" if is_v1 else "agent/getAuthenticatedExtendedCard"):
        return {
            "jsonrpc": "2.0",
            "id": payload.id,
            "result": {
                "name": "Authenticated A2A Agent",
                "description": "Extended agent card",
                "version": "1.0.0",
                "capabilities": {},
                "defaultInputModes": ["text/plain"],
                "defaultOutputModes": ["text/plain"],
                "skills": [],
            },
        }

    if method == ("GetTask" if is_v1 else "tasks/get"):
        task_id = params.get("id")
        task_data = tasks_db.get(task_id)
        if not task_data:
            return {
                "jsonrpc": "2.0",
                "id": payload.id,
                "error": {"code": -32602, "message": "Task not found"},
            }
        state = task_data["status"]["state"]
        result = {"id": task_id, "status": {"state": _task_state(state, is_v1)}}
        if state == "completed":
            if is_v1:
                result["artifacts"] = [{"parts": [{"text": task_data.get("text", "")}]}]
            else:
                result["text"] = task_data.get("text", "")
        if state in ("input-required", "auth-required"):
            question = task_data.get("question") or (
                "需要重新鉴权才能继续。" if state == "auth-required"
                else f"关于「{task_data['prompt']}」，能再补充一些细节吗？"
            )
            result["status"]["message"] = (
                {
                    "role": "ROLE_AGENT",
                    "messageId": f"msg_{uuid.uuid4().hex[:12]}",
                    "parts": [{"text": question}],
                }
                if is_v1
                else {
                    "kind": "message",
                    "role": "agent",
                    "parts": [{"kind": "text", "text": question}],
                }
            )
        return {"jsonrpc": "2.0", "id": payload.id, "result": result}

    if method == ("CancelTask" if is_v1 else "tasks/cancel"):
        task_id = params.get("id")
        task_data = tasks_db.get(task_id)
        if task_data:
            task_data["status"]["state"] = "canceled"
        return {
            "jsonrpc": "2.0",
            "id": payload.id,
            "result": {
                "id": task_id,
                "status": {"state": _task_state("canceled", is_v1)},
            },
        }

    return {
        "jsonrpc": "2.0",
        "id": payload.id,
        "error": {"code": -32601, "message": f"Method {method} not found"},
    }


def _task_state(state: str, is_v1: bool) -> str:
    if not is_v1:
        return state
    return {
        "submitted": "TASK_STATE_SUBMITTED",
        "working": "TASK_STATE_WORKING",
        "completed": "TASK_STATE_COMPLETED",
        "failed": "TASK_STATE_FAILED",
        "canceled": "TASK_STATE_CANCELED",
        "rejected": "TASK_STATE_REJECTED",
        "input-required": "TASK_STATE_INPUT_REQUIRED",
        "auth-required": "TASK_STATE_AUTH_REQUIRED",
    }.get(state, "TASK_STATE_UNSPECIFIED")


async def _advance_task(task_id: str, delay: float):
    await asyncio.sleep(delay)
    task = tasks_db.get(task_id)
    if not task:
        return
    final_state = task["final_state"]
    task["status"]["state"] = final_state
    if final_state == "completed":
        task["text"] = (
            f"【异步响应】你好！我是异步处理完成的 A2A 智能体。"
            f"针对你的问题：'{task['prompt']}'，我的计算结果已生成。"
        )
        if task.get("push"):
            await asyncio.to_thread(_deliver_push, task)
    # task-timeout scenario: final_state == "working", so this never fires and
    # the task is left in `working` forever, exercising the bridge's 60s deadline.


def _deliver_push(task: Dict[str, Any]):
    push = task["push"]
    body = json.dumps(
        {
            "id": task["id"],
            "status": {
                "state": _task_state(
                    task["status"]["state"], task.get("protocol_version") == "1.0"
                )
            },
            "artifacts": [{"parts": [{"text": task.get("text", "")}]}],
        }
    ).encode()
    request = urllib.request.Request(
        push["url"],
        data=body,
        headers={
            "Authorization": f"Bearer {push['token']}",
            "A2A-Version": task.get("protocol_version", "0.3"),
            "Content-Type": "application/json",
        },
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=5):
        pass


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8080)
