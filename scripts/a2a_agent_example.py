import uuid
import asyncio
from fastapi import FastAPI, Request
from fastapi.responses import JSONResponse
from pydantic import BaseModel
from typing import Dict, Any, List

app = FastAPI(title="A2A Agent Example Server")

# Temporary in-memory database for asynchronous tasks
tasks_db: Dict[str, Dict[str, Any]] = {}

class JsonRpcRequest(BaseModel):
    jsonrpc: str = "2.0"
    id: Any
    method: str
    params: Dict[str, Any] = {}

# 1. A2A Agent Card Discovery Endpoint
@app.get("/.well-known/agent-card.json")
def get_agent_card(request: Request):
    # Dynamically determine host URL
    base_url = str(request.base_url).rstrip("/")
    return {
        "id": "example-a2a-agent",
        "name": "A2A 示例智能体",
        "description": "一个用于演示 A2A 协议交互的示例智能体，支持同步和异步任务处理。",
        "protocolVersion": "0.3",
        "url": f"{base_url}/rpc",
        "preferredTransport": "JSONRPC",
        "version": "1.0.0",
        "capabilities": {
            "streaming": False,
            "pushNotifications": False,
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

# 2. JSON-RPC 2.0 Endpoint
@app.post("/rpc")
async def rpc_endpoint(payload: JsonRpcRequest):
    method = payload.method
    params = payload.params

    if method == "message/send":
        # Extract the user prompt
        message = params.get("message", {})
        parts = message.get("parts", [])
        prompt = ""
        for part in parts:
            if part.get("kind") == "text":
                prompt += part.get("text", "")
        
        # Decide whether to run Synchronously or Asynchronously
        # For demonstration, we run synchronously unless the prompt contains the word "async"
        is_async = "async" in prompt.lower()

        if not is_async:
            # --- Synchronous Path (Immediate Response) ---
            reply_text = f"【同步响应】你好！我是 A2A 智能体。我已收到你的请求，你问的是：'{prompt}'"
            return {
                "jsonrpc": "2.0",
                "id": payload.id,
                "result": {
                    "text": reply_text
                }
            }
        else:
            # --- Asynchronous Path (Task Polling) ---
            task_id = f"task_{uuid.uuid4().hex[:12]}"
            
            # Save task details in our in-memory DB with "working" state
            tasks_db[task_id] = {
                "id": task_id,
                "status": {"state": "working"},
                "prompt": prompt,
                "progress": 0
            }

            # Start a background job to simulate work and complete the task
            asyncio.create_task(simulate_background_task(task_id))

            return {
                "jsonrpc": "2.0",
                "id": payload.id,
                "result": {
                    "id": task_id,
                    "contextId": f"ctx_{uuid.uuid4().hex[:8]}"
                }
            }

    elif method == "tasks/get":
        task_id = params.get("id")
        if not task_id or task_id not in tasks_db:
            return JSONResponse(
                status_code=200,
                content={
                    "jsonrpc": "2.0",
                    "id": payload.id,
                    "error": {
                        "code": -32602,
                        "message": "Task not found"
                    }
                }
            )

        task_data = tasks_db[task_id]
        
        # Build response matching the A2A spec expectations
        response_result = {
            "id": task_id,
            "status": {
                "state": task_data["status"]["state"]
            }
        }
        if task_data["status"]["state"] == "completed":
            response_result["text"] = task_data["text"]

        return {
            "jsonrpc": "2.0",
            "id": payload.id,
            "result": response_result
        }

    else:
        # Method not found
        return JSONResponse(
            status_code=200,
            content={
                "jsonrpc": "2.0",
                "id": payload.id,
                "error": {
                    "code": -32601,
                    "message": f"Method {method} not found"
                }
            }
        )

async def simulate_background_task(task_id: str):
    # Simulate processing time (e.g., 3 seconds)
    await asyncio.sleep(3)
    
    if task_id in tasks_db:
        prompt = tasks_db[task_id]["prompt"]
        tasks_db[task_id]["status"]["state"] = "completed"
        tasks_db[task_id]["text"] = f"【异步响应】你好！我是异步处理完成的 A2A 智能体。针对你的问题：'{prompt}'，我的计算结果已生成。"

if __name__ == "__main__":
    import uvicorn
    # Run the server on port 8080
    uvicorn.run(app, host="0.0.0.0", port=8080)
