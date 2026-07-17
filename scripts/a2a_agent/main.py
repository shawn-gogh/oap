import uuid
import asyncio
from fastapi import FastAPI, Request, Header
from fastapi.responses import JSONResponse, PlainTextResponse, HTMLResponse
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
    "missing-name": "Agent card omits the required `name` field -> discover() returns 0 agents.",
    "no-identity": "Agent card has `name`/`description` but no `id`/`url` -> identity falls back to the endpoint; RPC calls then hit a URL with no handler.",
    "malformed-json": "Agent card endpoint returns 200 with a body that is not valid JSON -> ImportAgentsError::Decode.",
    "http-500": "Agent card endpoint returns HTTP 500 -> ImportAgentsError::Upstream.",
    "auth-required": "Agent card endpoint requires `Authorization: Bearer <api_key>`; wrong/missing key -> 401 Upstream error.",
    "high-risk": "Agent card carries high-risk raw fields (permissions/network/filesystem/secrets/...) -> normalize_agent() marks the import approval_required.",
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
    card = {
        "id": f"example-a2a-agent{('-' + scenario) if scenario else ''}",
        "name": "A2A 示例智能体",
        "description": "一个用于演示 A2A 协议交互的示例智能体，支持同步和异步任务处理。",
        "url": f"{base_url}{prefix}/rpc",
        "version": "1.0.0",
    }
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
async def rpc_endpoint(payload: JsonRpcRequest):
    return await _handle_rpc(None, payload)


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

    if scenario in ("task-fail", "task-input-required", "task-timeout", "rpc-error"):
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
async def scenario_rpc(scenario: str, payload: JsonRpcRequest):
    return await _handle_rpc(scenario, payload)


# --- Shared JSON-RPC handling ----------------------------------------------

async def _handle_rpc(scenario: Optional[str], payload: JsonRpcRequest):
    method = payload.method
    params = payload.params

    if method == "message/send":
        if scenario == "rpc-error":
            return {
                "jsonrpc": "2.0",
                "id": payload.id,
                "error": {"code": -32000, "message": "simulated agent-side failure"},
            }

        message = params.get("message", {})
        prompt = "".join(
            part.get("text", "") for part in message.get("parts", []) if part.get("kind") == "text"
        )

        resume_task_id = params.get("taskId")
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
            return {
                "jsonrpc": "2.0",
                "id": payload.id,
                "result": {
                    "id": resume_task_id,
                    "status": {"state": "completed"},
                    "text": task["text"],
                },
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
            return {
                "jsonrpc": "2.0",
                "id": payload.id,
                "result": {"id": task_id, "contextId": f"ctx_{uuid.uuid4().hex[:8]}"},
            }

        is_async = "async" in prompt.lower()
        if not is_async:
            reply_text = f"【同步响应】你好！我是 A2A 智能体。我已收到你的请求，你问的是：'{prompt}'"
            return {"jsonrpc": "2.0", "id": payload.id, "result": {"text": reply_text}}

        task_id = f"task_{uuid.uuid4().hex[:12]}"
        tasks_db[task_id] = {
            "id": task_id,
            "status": {"state": "working"},
            "prompt": prompt,
            "final_state": "completed",
        }
        asyncio.create_task(_advance_task(task_id, delay=3))
        return {
            "jsonrpc": "2.0",
            "id": payload.id,
            "result": {"id": task_id, "contextId": f"ctx_{uuid.uuid4().hex[:8]}"},
        }

    if method == "tasks/get":
        task_id = params.get("id")
        task_data = tasks_db.get(task_id)
        if not task_data:
            return {
                "jsonrpc": "2.0",
                "id": payload.id,
                "error": {"code": -32602, "message": "Task not found"},
            }
        state = task_data["status"]["state"]
        result = {"id": task_id, "status": {"state": state}}
        if state == "completed":
            result["text"] = task_data.get("text", "")
        if state in ("input-required", "auth-required"):
            question = task_data.get("question") or (
                "需要重新鉴权才能继续。" if state == "auth-required"
                else f"关于「{task_data['prompt']}」，能再补充一些细节吗？"
            )
            result["status"]["message"] = {
                "kind": "message",
                "role": "agent",
                "parts": [{"kind": "text", "text": question}],
            }
        return {"jsonrpc": "2.0", "id": payload.id, "result": result}

    if method == "tasks/cancel":
        task_id = params.get("id")
        task_data = tasks_db.get(task_id)
        if task_data:
            task_data["status"]["state"] = "canceled"
        return {"jsonrpc": "2.0", "id": payload.id, "result": {"id": task_id}}

    return {
        "jsonrpc": "2.0",
        "id": payload.id,
        "error": {"code": -32601, "message": f"Method {method} not found"},
    }


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
    # task-timeout scenario: final_state == "working", so this never fires and
    # the task is left in `working` forever, exercising the bridge's 60s deadline.


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8080)
