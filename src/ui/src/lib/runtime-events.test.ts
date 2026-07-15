import { describe, expect, it } from "vitest";

import type { RuntimeAgentEvent } from "./api";
import {
  mergeRuntimeEventList,
  runtimeEventsToMessages,
  runtimeSessionStatusFromMetadata,
  runtimeStatusFromEvents,
} from "./runtime-events";

const SID = "sess_1";

function userMessage(text: string, extra: Record<string, unknown> = {}): RuntimeAgentEvent {
  return { type: "user.message", content: [{ type: "text", text }], ...extra };
}

describe("mergeRuntimeEventList", () => {
  it("dedupes events by id", () => {
    const a: RuntimeAgentEvent = { id: "e1", type: "agent.message", text: "hi" };
    const merged = mergeRuntimeEventList([a], [{ ...a }, { id: "e2", type: "agent.message", text: "yo" }]);
    expect(merged).toHaveLength(2);
  });

  it("replaces the optimistic local user message with the server copy", () => {
    const local = userMessage("do the thing", { id: "local1", local: true });
    const server = userMessage("do the thing", { id: "srv1" });
    const merged = mergeRuntimeEventList([local], [server]);
    expect(merged).toHaveLength(1);
    expect(merged[0].id).toBe("srv1");
  });

  it("keeps a local user message whose text differs", () => {
    const local = userMessage("first", { id: "local1", local: true });
    const server = userMessage("second", { id: "srv1" });
    const merged = mergeRuntimeEventList([local], [server]);
    expect(merged).toHaveLength(2);
  });
});

describe("runtimeEventsToMessages", () => {
  it("converts a user turn and streamed assistant text", () => {
    const events: RuntimeAgentEvent[] = [
      userMessage("hello", { id: "u1" }),
      { id: "t1", type: "session.status_running" },
      { id: "d1", type: "content_block_delta", delta: { text: "Hel" } },
      { id: "d2", type: "content_block_delta", delta: { text: "lo!" } },
      { id: "s1", type: "session.status_idle" },
    ];
    const messages = runtimeEventsToMessages(SID, events, "idle");
    expect(messages).toHaveLength(2);
    expect(messages[0].info.role).toBe("user");
    expect(messages[1].info.role).toBe("assistant");
    const textPart = messages[1].parts.find((part) => part.type === "text");
    expect(textPart && "text" in textPart ? textPart.text : "").toBe("Hello!");
    expect(messages[1].info.finish).toBe("stop");
  });

  it("routes thinking events into a thinking part", () => {
    const events: RuntimeAgentEvent[] = [
      { id: "t1", type: "session.status_running" },
      { id: "th1", type: "agent.thinking", text: "pondering" },
      { id: "d1", type: "agent.message", text: "answer" },
    ];
    const [assistant] = runtimeEventsToMessages(SID, events, "busy");
    const kinds = assistant.parts.map((part) => part.type);
    expect(kinds).toContain("thinking");
    expect(kinds).toContain("text");
  });

  it("merges tool_call and tool_result into one tool part", () => {
    const events: RuntimeAgentEvent[] = [
      { id: "t1", type: "session.status_running" },
      { id: "c1", type: "tool_call", tool_use_id: "tool_9", name: "bash", input: { cmd: "ls" } },
      { id: "r1", type: "tool_result", tool_use_id: "tool_9", output: "files" },
    ];
    const [assistant] = runtimeEventsToMessages(SID, events, "busy");
    const toolParts = assistant.parts.filter((part) => part.type === "tool");
    expect(toolParts).toHaveLength(1);
    const tool = toolParts[0];
    if (tool.type !== "tool") throw new Error("expected tool part");
    expect(tool.tool).toBe("bash");
    expect(tool.state.status).toBe("completed");
    expect(tool.state.output).toBe("files");
  });

  it("keeps a late tool result on the turn that started the tool", () => {
    const events: RuntimeAgentEvent[] = [
      userMessage("first", { id: "u1" }),
      { id: "c1", type: "agent.tool_use", name: "bash", status: "running", occurred_at: 1000 },
      userMessage("second", { id: "u2" }),
      { id: "r1", type: "agent.tool_result", tool_use_id: "c1", name: "bash", status: "aborted", occurred_at: 2000 },
    ];
    const messages = runtimeEventsToMessages(SID, events, "idle");
    const firstAssistant = messages.find((message) =>
      message.info.role === "assistant" && message.parts.some((part) => part.type === "tool"),
    );
    const tool = firstAssistant?.parts.find((part) => part.type === "tool");
    expect(tool?.type === "tool" ? tool.state.status : "").toBe("aborted");
    expect(tool?.type === "tool" ? tool.state.startedAt : undefined).toBe(1000);
    expect(tool?.type === "tool" ? tool.state.completedAt : undefined).toBe(2000);
  });

  it("preserves timeout metadata for a recoverable tool error", () => {
    const [assistant] = runtimeEventsToMessages(SID, [
      { id: "c1", type: "agent.tool_use", name: "bash", status: "running", occurred_at: 1000 },
      {
        id: "r1",
        type: "agent.tool_result",
        tool_use_id: "c1",
        name: "bash",
        status: "timed_out",
        error_code: "tool_timeout",
        error_message: "命令运行超时",
        occurred_at: 601000,
      },
    ], "idle");
    const tool = assistant.parts.find((part) => part.type === "tool");
    expect(tool?.type === "tool" ? tool.state.status : "").toBe("timed_out");
    expect(tool?.type === "tool" ? tool.state.errorCode : "").toBe("tool_timeout");
    expect(tool?.type === "tool" ? tool.state.errorMessage : "").toBe("命令运行超时");
  });

  it("recognizes timeout metadata from sessions created before structured tool errors", () => {
    const [assistant] = runtimeEventsToMessages(SID, [
      { id: "c1", type: "agent.tool_use", name: "bash", status: "running" },
      {
        id: "r1",
        type: "agent.tool_result",
        tool_use_id: "c1",
        name: "bash",
        output:
          "partial output\n<shell_metadata>shell tool terminated command after exceeding timeout 600000 ms.</shell_metadata>",
      },
    ], "idle");
    const tool = assistant.parts.find((part) => part.type === "tool");
    expect(tool?.type === "tool" ? tool.state.status : "").toBe("timed_out");
    expect(tool?.type === "tool" ? tool.state.errorMessage : "").toBe("命令运行超时，已停止执行");
  });

  it("appends a pending assistant message while busy", () => {
    const events: RuntimeAgentEvent[] = [userMessage("hello", { id: "u1" })];
    const messages = runtimeEventsToMessages(SID, events, "busy");
    expect(messages.at(-1)?.info.role).toBe("assistant");
  });

  it("renders session.error as a finished assistant message", () => {
    const events: RuntimeAgentEvent[] = [
      { id: "e1", type: "session.error", error: { message: "boom" } },
    ];
    const [assistant] = runtimeEventsToMessages(SID, events, "idle");
    const textPart = assistant.parts.find((part) => part.type === "text");
    expect(textPart && "text" in textPart ? textPart.text : "").toContain("boom");
    expect(assistant.info.finish).toBe("stop");
  });
});

describe("status derivation", () => {
  it("derives busy from a turn start and idle from a terminal event", () => {
    expect(runtimeStatusFromEvents([{ id: "1", type: "session.status_running" }])).toBe("busy");
    expect(
      runtimeStatusFromEvents([
        { id: "1", type: "session.status_running" },
        { id: "2", type: "session.status_idle" },
      ]),
    ).toBe("idle");
  });

  it("returns null when no status-bearing events exist", () => {
    expect(runtimeStatusFromEvents([{ id: "1", type: "agent.message", text: "x" }])).toBeNull();
  });

  it("maps session metadata to busy/idle", () => {
    expect(runtimeSessionStatusFromMetadata("running", undefined)).toBe("busy");
    expect(runtimeSessionStatusFromMetadata("idle", undefined)).toBe("idle");
    expect(runtimeSessionStatusFromMetadata(undefined, "run_123")).toBe("busy");
    expect(runtimeSessionStatusFromMetadata(undefined, undefined)).toBe("idle");
  });
});
