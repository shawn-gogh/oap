from typing import Literal
from uuid import uuid4

from fastapi import FastAPI
from pydantic import BaseModel, Field


app = FastAPI(
    title="Generic Evidence Agent API",
    description=(
        "A conventional third-party REST agent. Its contract is intentionally "
        "designed independently of LAP and contains no x-lap-runtime extension."
    ),
    version="1.0.0",
)


class Message(BaseModel):
    role: Literal["user", "assistant", "system"]
    content: str


class RunRequest(BaseModel):
    messages: list[Message]
    metadata: dict[str, str] = Field(default_factory=dict)


class RunResponse(BaseModel):
    run_id: str
    status: Literal["completed"]
    messages: list[Message]


@app.get("/health")
def health() -> dict[str, str]:
    return {"status": "ok"}


@app.post("/api/v1/runs", response_model=RunResponse)
def run_agent(request: RunRequest) -> RunResponse:
    latest = next(
        (message.content for message in reversed(request.messages) if message.role == "user"),
        "",
    )
    return RunResponse(
        run_id=f"run_{uuid4().hex}",
        status="completed",
        messages=[
            Message(
                role="assistant",
                content=(
                    "I received the evidence request: "
                    f"{latest}. Verify dates, identities, and primary sources."
                ),
            )
        ],
    )
