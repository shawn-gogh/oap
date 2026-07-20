import os
from typing import Any, Literal
from uuid import uuid4

from crewai import Agent, BaseLLM, Crew, LLM, Process, Task
from fastapi import FastAPI
from pydantic import BaseModel


app = FastAPI(
    title="Self-hosted CrewAI research crew",
    description=(
        "A normal self-hosted CrewAI application API. It is not an emulation "
        "of CrewAI AMP and does not expose AMP's deployment-specific /inputs route."
    ),
    version="1.0.0",
)


class LocalEvidenceLLM(BaseLLM):
    def call(
        self,
        messages: str | list[dict[str, Any]],
        tools: list[dict[str, Any]] | None = None,
        callbacks: list[Any] | None = None,
        available_functions: dict[str, Any] | None = None,
        from_task: Task | None = None,
        from_agent: Agent | None = None,
        response_model: type[BaseModel] | None = None,
    ) -> str:
        return (
            "Final Answer: The claim is not yet verified. Collect the primary "
            "incident timeline, timestamps, actor identities, and independent "
            "corroboration before drawing a conclusion."
        )


def build_crew(topic: str, llm_mode: str) -> Crew:
    if llm_mode == "gateway":
        llm = LLM(
            model=os.environ["CREWAI_MODEL"],
            base_url=os.environ["OPENAI_API_BASE"],
            api_key=os.environ["OPENAI_API_KEY"],
        )
    else:
        llm = LocalEvidenceLLM(model="local/evidence-review")
    researcher = Agent(
        role="Evidence researcher",
        goal="Separate verified facts from unsupported claims",
        backstory="You are a careful analyst who prefers primary sources.",
        llm=llm,
        verbose=False,
    )
    task = Task(
        description=f"Review this topic and state what evidence is still needed: {topic}",
        expected_output="A concise evidence review with explicit uncertainties.",
        agent=researcher,
    )
    return Crew(agents=[researcher], tasks=[task], process=Process.sequential, verbose=False)


class KickoffRequest(BaseModel):
    topic: str
    llm_mode: Literal["local", "gateway"] = "local"


@app.get("/health")
def health() -> dict[str, str]:
    return {"status": "ok"}


@app.get("/api/v1/agents")
def agents() -> dict:
    return {
        "items": [
            {
                "id": "evidence-researcher",
                "role": "Evidence researcher",
                "goal": "Separate verified facts from unsupported claims",
            }
        ]
    }


@app.post("/api/v1/kickoffs")
def kickoff(request: KickoffRequest) -> dict[str, str]:
    result = build_crew(request.topic, request.llm_mode).kickoff()
    return {
        "id": f"kickoff_{uuid4().hex}",
        "status": "completed",
        "output": str(result),
    }
