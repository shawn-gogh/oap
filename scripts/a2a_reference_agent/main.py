#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
A2A 1.0 参考智能体（Reference Agent）—— 按 Agent2Agent 协议规范实现，非模拟。

用途：检验本平台「纳管 A2A 智能体」的功能完备性（发现/导入 + 运行/流式 + 任务生命周期）。
本 agent 严格按 A2A 规范提供 Agent Card 和 JSON-RPC 接口，不针对本平台做任何定制。
它做的是「真实且可核对」的工作：对输入文本做统计分析并原样回带，便于确认数据真的走通了整条链路。

协议要点（依据 a2a-protocol.org 与 a2aproject/A2A 规范）：
  - 发现：GET /.well-known/agent-card.json 返回 A2A 1.0 Agent Card
  - 传输：JSON-RPC 2.0 over HTTP，POST 到 supportedInterfaces[0].url
  - 方法：本 agent 同时接受两套命名，便于对照测试：
      * 经典 JSON-RPC 绑定：message/send, message/stream, tasks/get, tasks/cancel
      * v1.0 proto 绑定：      SendMessage, SendStreamingMessage, GetTask, CancelTask
  - 响应形状按“请求所用的绑定”自动匹配：
      * proto 请求  -> role=ROLE_AGENT, state=TASK_STATE_COMPLETED, part 裸 {text}
      * 经典请求    -> role=agent,      state=completed,            part {kind:text,text}
    平台解析器两者都接受；分别回不同形状是为了如实暴露两条路各自是否走得通。

运行：
  python3 main.py                # 监听 0.0.0.0:8080
  A2A_PORT=9300 python3 main.py  # 自定义端口
  # 或用 Docker（见同目录 Dockerfile / compose 服务 a2a-reference）

导入到 OAP：来源类型选 A2A，服务地址填本 agent 的根地址（不带 /.well-known）：
  http://a2a-reference:8080
注意：导入由 lap 在容器内发起，且平台会拒绝 loopback/link-local 地址，
因此必须用 compose 服务名访问（如上），不能填 localhost / 127.0.0.1。
"""

import json
import os
import re
import sys
import time
import uuid
import threading
import urllib.request
import urllib.error
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT = int(os.environ.get("A2A_PORT", "8080"))

# 真实模型接入（可选）：设置 A2A_MODEL 后，agent 把用户消息转发给一个
# Anthropic /v1/messages 兼容网关，返回模型的真实回答；不设则回退到确定性
# 文本分析（便于纯协议测试）。默认指向本平台的 lap 网关。
A2A_MODEL = os.environ.get("A2A_MODEL", "").strip()
A2A_LLM_BASE_URL = os.environ.get("A2A_LLM_BASE_URL", "http://lap:4000").rstrip("/")
A2A_LLM_API_KEY = os.environ.get("A2A_LLM_API_KEY", "sk-local")
A2A_SYSTEM = os.environ.get(
    "A2A_SYSTEM",
    "你是一个通过 A2A 协议对外提供服务的智能体助手。请简洁、准确地回答用户。",
)
A2A_MAX_TOKENS = int(os.environ.get("A2A_MAX_TOKENS", "1024"))
# 对外公布的基址：Agent Card 里的 interface url 必须是导入方能回连的地址。
# 默认用请求 Host 头动态推导；也可用 A2A_PUBLIC_BASE 固定（如经反代/内网服务名）。
PUBLIC_BASE = os.environ.get("A2A_PUBLIC_BASE", "").rstrip("/")

# 任务存储：task_id -> task 字典。GetTask/CancelTask 从这里取。
TASKS = {}
TASKS_LOCK = threading.Lock()


def now_iso():
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


# --------------------------------------------------------------------------- #
# Agent Card（A2A 1.0）
# --------------------------------------------------------------------------- #
def agent_card(base_url):
    """严格的 A2A 1.0 Agent Card。故意不声明任何 required 扩展，因此可被正常导入。
    卡片内容随运行模式（真实模型 / 确定性分析）如实变化。"""
    if A2A_MODEL:
        name = "对话参考智能体"
        description = (
            f"A2A 1.0 参考实现，背后由真实模型 `{A2A_MODEL}` 驱动。"
            "接收用户消息并返回模型的真实回答，用于端到端验证 A2A 的发现、导入、运行与流式能力。"
        )
        skill = {
            "id": "chat",
            "name": "对话问答",
            "description": f"由 {A2A_MODEL} 生成的自然语言回答。",
            "tags": ["chat", "llm", "reference"],
            "examples": ["用一句话解释什么是 A2A 协议", "帮我把这段话翻译成英文：你好世界"],
            "inputModes": ["text/plain"],
            "outputModes": ["text/plain"],
        }
    else:
        name = "文本分析参考智能体"
        description = (
            "A2A 1.0 参考实现。接收一段文本，返回字符数、词数、行数以及反转后的文本。"
            "用于端到端验证 A2A 的发现、导入、运行与流式能力。"
        )
        skill = {
            "id": "text-analysis",
            "name": "文本分析",
            "description": "统计字符/词/行数并返回反转文本。",
            "tags": ["text", "analysis", "reference"],
            "examples": ["分析这段话：你好，世界", "count the words in: hello world"],
            "inputModes": ["text/plain"],
            "outputModes": ["text/plain"],
        }
    return {
        "protocolVersion": "1.0",
        "name": name,
        "description": description,
        "version": "1.0.0",
        "provider": {
            "organization": "OAP Reference",
            "url": base_url,
        },
        # v1.0：通过 supportedInterfaces 声明传输，首项为首选。
        "supportedInterfaces": [
            {
                "url": f"{base_url}/a2a/v1",
                "protocolBinding": "JSONRPC",
                "protocolVersion": "1.0",
            }
        ],
        "capabilities": {
            "streaming": True,
            "pushNotifications": False,
            "extendedAgentCard": False,
            "extensions": [],
        },
        "defaultInputModes": ["text/plain"],
        "defaultOutputModes": ["text/plain"],
        "skills": [skill],
    }


# --------------------------------------------------------------------------- #
# 真实业务逻辑：文本分析（确定性、可核对）
# --------------------------------------------------------------------------- #
def analyze(text):
    text = text or ""
    words = re.findall(r"\S+", text)
    lines = text.split("\n") if text else []
    non_ws = re.sub(r"\s", "", text)
    reversed_text = text[::-1]
    return "\n".join([
        "【文本分析结果】",
        "字符数（含空白）：{}".format(len(text)),
        "字符数（不含空白）：{}".format(len(non_ws)),
        "词数：{}".format(len(words)),
        "行数：{}".format(len(lines)),
        "反转文本：{}".format(reversed_text),
    ])


def call_model(text):
    """把用户消息转发给 Anthropic /v1/messages 兼容网关，返回模型的真实回答。"""
    payload = json.dumps({
        "model": A2A_MODEL,
        "max_tokens": A2A_MAX_TOKENS,
        "system": A2A_SYSTEM,
        "messages": [{"role": "user", "content": text or ""}],
    }).encode("utf-8")
    req = urllib.request.Request(
        f"{A2A_LLM_BASE_URL}/v1/messages",
        data=payload,
        method="POST",
        headers={
            "content-type": "application/json",
            "authorization": f"Bearer {A2A_LLM_API_KEY}",
            "anthropic-version": "2023-06-01",
        },
    )
    with urllib.request.urlopen(req, timeout=120) as resp:
        body = json.loads(resp.read().decode("utf-8"))
    parts = body.get("content") or []
    out = "".join(p.get("text", "") for p in parts if isinstance(p, dict))
    return out.strip() or "（模型返回了空内容）"


def respond(text):
    """按配置分派：设置了 A2A_MODEL 就走真实模型，否则走确定性分析。
    模型调用失败时回退到分析，并附带错误说明，保证协议链路仍然完成。"""
    if not A2A_MODEL:
        return analyze(text)
    try:
        return call_model(text)
    except (urllib.error.URLError, urllib.error.HTTPError, TimeoutError, ValueError) as e:
        sys.stderr.write(f"[a2a] 模型调用失败，回退到确定性分析：{e}\n")
        sys.stderr.flush()
        return "（模型调用失败，以下为本地分析回退）\n" + analyze(text)


# --------------------------------------------------------------------------- #
# 请求解析：兼容两套 role/part 形状
# --------------------------------------------------------------------------- #
def extract_text(message):
    """从 A2A Message 中取出纯文本，兼容 {text} / {kind:text,text} / {data} 等。"""
    if not isinstance(message, dict):
        return ""
    parts = message.get("parts") or []
    out = []
    for p in parts:
        if not isinstance(p, dict):
            continue
        if isinstance(p.get("text"), str):
            out.append(p["text"])
        elif isinstance(p.get("data"), (dict, list)):
            out.append(json.dumps(p["data"], ensure_ascii=False))
    return "\n".join(out)


def is_proto_method(method):
    """PascalCase 即 v1.0 proto 绑定；带斜杠即经典 JSON-RPC 绑定。"""
    return "/" not in method


def make_part(text, proto):
    return {"text": text} if proto else {"kind": "text", "text": text}


def make_task(task_id, context_id, state_terminal, text, proto):
    """构造一个 A2A Task。state_terminal 为语义状态（completed/working/...）。"""
    if proto:
        role = "ROLE_AGENT"
        state = {
            "completed": "TASK_STATE_COMPLETED",
            "working": "TASK_STATE_WORKING",
            "submitted": "TASK_STATE_SUBMITTED",
            "canceled": "TASK_STATE_CANCELED",
            "failed": "TASK_STATE_FAILED",
        }[state_terminal]
    else:
        role = "agent"
        state = state_terminal
    task = {
        "id": task_id,
        "contextId": context_id,
        "status": {"state": state, "timestamp": now_iso()},
        "artifacts": [],
        "history": [],
    }
    if not proto:
        task["kind"] = "task"
    if state_terminal == "completed" and text is not None:
        task["artifacts"] = [
            {
                "artifactId": f"art-{uuid.uuid4().hex[:8]}",
                "name": "analysis",
                "parts": [make_part(text, proto)],
            }
        ]
    return task


# --------------------------------------------------------------------------- #
# JSON-RPC 方法处理
# --------------------------------------------------------------------------- #
def handle_send(params, proto):
    message = params.get("message") or {}
    context_id = message.get("contextId") or f"ctx-{uuid.uuid4().hex[:8]}"
    task_id = message.get("taskId") or f"task-{uuid.uuid4().hex[:12]}"
    text = extract_text(message)
    report = respond(text)
    task = make_task(task_id, context_id, "completed", report, proto)
    with TASKS_LOCK:
        TASKS[task_id] = task
    return task


def handle_get(params, proto):
    task_id = params.get("id") or params.get("taskId") or (params.get("name") or "").split("/")[-1]
    with TASKS_LOCK:
        task = TASKS.get(task_id)
    if task is None:
        raise JsonRpcError(-32001, f"task not found: {task_id}")
    return task


def handle_cancel(params, proto):
    task_id = params.get("id") or params.get("taskId") or (params.get("name") or "").split("/")[-1]
    with TASKS_LOCK:
        task = TASKS.get(task_id)
        if task is None:
            raise JsonRpcError(-32001, f"task not found: {task_id}")
        task["status"] = {
            "state": "TASK_STATE_CANCELED" if proto else "canceled",
            "timestamp": now_iso(),
        }
    return task


class JsonRpcError(Exception):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code
        self.message = message


# --------------------------------------------------------------------------- #
# 流式：SendStreamingMessage / message/stream -> SSE
# 事件序列：working 状态 -> 含结果的 artifact -> completed 状态
# 每个 SSE 事件的 data 是一个完整的 JSON-RPC result 帧。
# --------------------------------------------------------------------------- #
def stream_frames(request_id, params, proto):
    message = params.get("message") or {}
    context_id = message.get("contextId") or f"ctx-{uuid.uuid4().hex[:8]}"
    task_id = message.get("taskId") or f"task-{uuid.uuid4().hex[:12]}"
    text = extract_text(message)
    report = respond(text)

    working = make_task(task_id, context_id, "working", None, proto)
    with TASKS_LOCK:
        TASKS[task_id] = working
    yield rpc_result(request_id, wrap_result(working, "SendStreamingMessage", proto))

    time.sleep(0.3)  # 让“进行中”状态可被观察到

    completed = make_task(task_id, context_id, "completed", report, proto)
    with TASKS_LOCK:
        TASKS[task_id] = completed
    yield rpc_result(request_id, wrap_result(completed, "SendStreamingMessage", proto))


def rpc_result(request_id, result):
    return {"jsonrpc": "2.0", "id": request_id, "result": result}


def wrap_result(task, method, proto):
    """按 A2A v1.0 规范包装响应：SendMessage/SendStreamingMessage 的 result 是
    一个 oneof，Task 放在 `task` 字段下（`{"result": {"task": {...}}}`）。
    GetTask/CancelTask 直接返回 Task 本身。经典 JSON-RPC 绑定（v0.3）一律裸 Task。"""
    if proto and method in ("SendMessage", "SendStreamingMessage"):
        return {"task": task}
    return task


# --------------------------------------------------------------------------- #
# HTTP 层
# --------------------------------------------------------------------------- #
class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def _base_url(self):
        if PUBLIC_BASE:
            return PUBLIC_BASE
        host = self.headers.get("Host") or f"localhost:{PORT}"
        return f"http://{host}"

    def _send_json(self, obj, status=200):
        body = json.dumps(obj, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path.rstrip("/") in ("/.well-known/agent-card.json".rstrip("/"),) or \
           self.path.startswith("/.well-known/agent-card.json"):
            self._send_json(agent_card(self._base_url()))
            return
        if self.path.startswith("/healthz"):
            self._send_json({"status": "ok"})
            return
        if self.path in ("/", ""):
            self._send_json({
                "agent": "A2A 1.0 文本分析参考智能体",
                "card": "/.well-known/agent-card.json",
                "rpc": "/a2a/v1",
            })
            return
        self._send_json({"error": "not found"}, status=404)

    def do_POST(self):
        length = int(self.headers.get("Content-Length", "0") or "0")
        raw = self.rfile.read(length) if length else b""
        try:
            payload = json.loads(raw.decode("utf-8"))
        except Exception:
            self._send_json({"jsonrpc": "2.0", "id": None,
                             "error": {"code": -32700, "message": "parse error"}}, status=400)
            return

        request_id = payload.get("id")
        method = payload.get("method") or ""
        params = payload.get("params") or {}
        proto = is_proto_method(method)

        # 可观测：把平台实际发来的方法与角色打到日志，便于对照规范。
        role = (params.get("message") or {}).get("role")
        sys.stderr.write(f"[a2a] <- method={method!r} proto_binding={proto} role={role!r}\n")
        sys.stderr.flush()

        send_methods = ("message/send", "SendMessage")
        stream_methods = ("message/stream", "SendStreamingMessage")
        get_methods = ("tasks/get", "GetTask")
        cancel_methods = ("tasks/cancel", "CancelTask")

        try:
            if method in stream_methods:
                self._stream(request_id, params, proto)
                return
            if method in send_methods:
                result = wrap_result(handle_send(params, proto), method, proto)
            elif method in get_methods:
                result = wrap_result(handle_get(params, proto), method, proto)
            elif method in cancel_methods:
                result = wrap_result(handle_cancel(params, proto), method, proto)
            else:
                self._send_json({"jsonrpc": "2.0", "id": request_id,
                                 "error": {"code": -32601,
                                           "message": f"method not found: {method}"}})
                return
        except JsonRpcError as e:
            self._send_json({"jsonrpc": "2.0", "id": request_id,
                             "error": {"code": e.code, "message": e.message}})
            return

        self._send_json(rpc_result(request_id, result))

    def _stream(self, request_id, params, proto):
        # `Connection: close` + `close_connection = True`：终态事件写完后关闭 socket，
        # 让读取方（含平台的 call_a2a_stream 与 curl -N）据此判定流结束。
        # 否则 HTTP/1.1 keep-alive 下无 Content-Length，读取方会一直等待。
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream; charset=utf-8")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("Connection", "close")
        self.end_headers()
        for frame in stream_frames(request_id, params, proto):
            data = json.dumps(frame, ensure_ascii=False)
            self.wfile.write(f"event: message\ndata: {data}\n\n".encode("utf-8"))
            self.wfile.flush()
        self.close_connection = True

    def log_message(self, *args):
        pass  # 用我们自己的 stderr 日志


def main():
    server = ThreadingHTTPServer(("0.0.0.0", PORT), Handler)
    sys.stderr.write(
        f"[a2a] A2A 1.0 参考智能体已启动： http://0.0.0.0:{PORT}\n"
        f"[a2a] Agent Card： http://0.0.0.0:{PORT}/.well-known/agent-card.json\n"
    )
    sys.stderr.flush()
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        server.shutdown()


if __name__ == "__main__":
    main()
