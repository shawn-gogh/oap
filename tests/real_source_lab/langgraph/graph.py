from langchain_core.messages import AIMessage
from langgraph.graph import END, START, MessagesState, StateGraph


def inspect_evidence(state: MessagesState) -> dict:
    latest = state["messages"][-1].content if state["messages"] else ""
    return {
        "messages": [
            AIMessage(
                content=(
                    "Evidence assistant received the request. "
                    f"Input length: {len(str(latest))}. "
                    "Provide timestamps and primary sources before drawing a conclusion."
                )
            )
        ]
    }


builder = StateGraph(MessagesState)
builder.add_node("inspect_evidence", inspect_evidence)
builder.add_edge(START, "inspect_evidence")
builder.add_edge("inspect_evidence", END)
graph = builder.compile()
