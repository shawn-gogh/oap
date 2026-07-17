"""A real A2A agent backed by the opencode runtime.

Protocol side: official `a2a-sdk` (JSON-RPC + SSE streaming, agent card at
/.well-known/agent-card.json). Execution side: each A2A context maps to a
dedicated workspace directory where `opencode run` keeps genuine session
state, so multi-turn conversations really continue the same opencode session.
"""

import asyncio
import logging
import os
import re
import uuid

import uvicorn

from a2a.server.agent_execution import AgentExecutor, RequestContext
from a2a.server.apps import A2AStarletteApplication
from a2a.server.events import EventQueue
from a2a.server.request_handlers import DefaultRequestHandler
from a2a.server.tasks import InMemoryTaskStore, TaskUpdater
from a2a.types import (
    AgentCapabilities,
    AgentCard,
    AgentSkill,
    Part,
    TextPart,
)
from a2a.utils import new_agent_text_message, new_task

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(name)s %(levelname)s %(message)s")
log = logging.getLogger("opencode-a2a")

HOST = os.environ.get("A2A_HOST", "0.0.0.0")
PORT = int(os.environ.get("A2A_PORT", "9200"))
PUBLIC_URL = os.environ.get("A2A_PUBLIC_URL", f"http://localhost:{PORT}/")
MODEL = os.environ.get("OPENCODE_MODEL", "")  # e.g. "anthropic/claude-sonnet-5"
WORKSPACES = os.environ.get("WORKSPACES_DIR", "/workspaces")
RUN_TIMEOUT = float(os.environ.get("OPENCODE_RUN_TIMEOUT", "300"))

ANSI_RE = re.compile(r"\x1b\[[0-9;?]*[a-zA-Z]|\x1b\][^\x07]*\x07")


class OpencodeExecutor(AgentExecutor):
    """Runs each user message through `opencode run` inside a per-context workspace."""

    def __init__(self) -> None:
        self._locks: dict[str, asyncio.Lock] = {}
        self._procs: dict[str, asyncio.subprocess.Process] = {}

    def _workspace(self, context_id: str) -> tuple[str, bool]:
        safe = re.sub(r"[^a-zA-Z0-9_-]", "_", context_id)[:64]
        path = os.path.join(WORKSPACES, safe or uuid.uuid4().hex)
        first_turn = not os.path.isdir(path)
        os.makedirs(path, exist_ok=True)
        return path, first_turn

    async def execute(self, context: RequestContext, event_queue: EventQueue) -> None:
        task = context.current_task
        if task is None:
            task = new_task(context.message)
            await event_queue.enqueue_event(task)
        updater = TaskUpdater(event_queue, task.id, task.context_id)

        user_text = context.get_user_input()
        if not user_text.strip():
            await updater.reject(new_agent_text_message("Empty message.", task.context_id, task.id))
            return

        workspace, first_turn = self._workspace(task.context_id)
        lock = self._locks.setdefault(task.context_id, asyncio.Lock())

        await updater.start_work(
            new_agent_text_message("opencode is working on it...", task.context_id, task.id)
        )

        cmd = ["opencode", "run", "--dir", workspace]
        if MODEL:
            cmd += ["--model", MODEL]
        if not first_turn:
            cmd += ["--continue"]  # resume the last opencode session in this workspace
        cmd += [user_text]

        log.info("task=%s ctx=%s ws=%s first_turn=%s", task.id, task.context_id, workspace, first_turn)

        async with lock:
            proc = await asyncio.create_subprocess_exec(
                *cmd,
                cwd=workspace,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
                env={**os.environ, "NO_COLOR": "1", "TERM": "dumb", "CI": "1", "PWD": workspace},
            )
            self._procs[task.id] = proc
            try:
                stdout, stderr = await asyncio.wait_for(proc.communicate(), timeout=RUN_TIMEOUT)
            except asyncio.TimeoutError:
                proc.kill()
                await updater.failed(
                    new_agent_text_message(
                        f"opencode run timed out after {RUN_TIMEOUT:.0f}s", task.context_id, task.id
                    )
                )
                return
            finally:
                self._procs.pop(task.id, None)

        out = ANSI_RE.sub("", stdout.decode(errors="replace")).strip()
        err = ANSI_RE.sub("", stderr.decode(errors="replace")).strip()

        if proc.returncode != 0:
            log.error("opencode failed rc=%s stderr=%s", proc.returncode, err[-2000:])
            await updater.failed(
                new_agent_text_message(
                    f"opencode exited with code {proc.returncode}: {err[-1500:] or out[-1500:] or 'no output'}",
                    task.context_id,
                    task.id,
                )
            )
            return

        answer = out or "(opencode produced no output)"
        await updater.add_artifact(
            [Part(root=TextPart(text=answer))], name="response", artifact_id=uuid.uuid4().hex
        )
        await updater.complete(new_agent_text_message(answer, task.context_id, task.id))

    async def cancel(self, context: RequestContext, event_queue: EventQueue) -> None:
        task = context.current_task
        if task and (proc := self._procs.get(task.id)):
            proc.kill()
        if task:
            updater = TaskUpdater(event_queue, task.id, task.context_id)
            await updater.cancel()


def agent_card() -> AgentCard:
    return AgentCard(
        name="opencode-dev-agent",
        description=(
            "A general-purpose software engineering agent powered by the opencode "
            "runtime. It can answer questions, write and edit code, and run shell "
            "commands inside its own workspace. Multi-turn: messages sharing an A2A "
            "context continue the same opencode session."
        ),
        url=PUBLIC_URL,
        version="1.0.0",
        default_input_modes=["text/plain"],
        default_output_modes=["text/plain"],
        capabilities=AgentCapabilities(streaming=True, push_notifications=False),
        skills=[
            AgentSkill(
                id="code",
                name="Software engineering",
                description="Write, edit, review and explain code; run commands in a sandboxed workspace.",
                tags=["coding", "shell", "opencode"],
                examples=[
                    "Write a python script that parses a CSV and prints column stats",
                    "Explain what this regex does: ^(?=.*[A-Z]).{8,}$",
                ],
            ),
            AgentSkill(
                id="chat",
                name="General Q&A",
                description="Answer general questions conversationally.",
                tags=["chat"],
                examples=["Summarize the tradeoffs between SSE and WebSockets"],
            ),
        ],
    )


def main() -> None:
    handler = DefaultRequestHandler(
        agent_executor=OpencodeExecutor(),
        task_store=InMemoryTaskStore(),
    )
    app = A2AStarletteApplication(agent_card=agent_card(), http_handler=handler)
    log.info("A2A agent listening on %s:%s (public url %s, model=%s)", HOST, PORT, PUBLIC_URL, MODEL or "<opencode default>")
    uvicorn.run(app.build(), host=HOST, port=PORT)


if __name__ == "__main__":
    main()
