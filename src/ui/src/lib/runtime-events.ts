import type { RuntimeAgentEvent } from "@/lib/api";
import type { HarnessMessage } from "@/lib/types";

function runtimeTextValue(value: unknown): string {
  if (typeof value === "string") return value;
  if (Array.isArray(value)) {
    return value.map(runtimeTextValue).join("");
  }
  if (!value || typeof value !== "object") return "";
  const record = value as Record<string, unknown>;
  return [
    record.text,
    record.thinking,
    record.content,
    record.delta,
    record.content_block,
  ]
    .map(runtimeTextValue)
    .join("");
}

function runtimeEventText(ev: RuntimeAgentEvent): string {
  return runtimeTextValue(ev.text ?? ev.delta ?? ev.content ?? ev.content_block);
}

export function normalizedRuntimeEventType(ev: RuntimeAgentEvent): string {
  const type = ev.type;
  return typeof type === "string" ? type : "";
}

function runtimeEventPartKind(ev: RuntimeAgentEvent): "text" | "thinking" {
  const part = ev.part;
  if (part && typeof part === "object") {
    const type = (part as { type?: unknown }).type;
    if (type === "thinking" || type === "reasoning") return "thinking";
  }
  const field = ev.field;
  if (field === "thinking" || field === "reasoning") return "thinking";
  const type = ev.type;
  if (type === "thinking_back" || type === "agent.thinking" || type === "agent.reasoning") {
    return "thinking";
  }
  return "text";
}

export function runtimeErrorMessage(ev: RuntimeAgentEvent): string {
  const error = ev.error;
  if (typeof error === "string") return error;
  if (error && typeof error === "object") {
    const message = (error as { message?: unknown }).message;
    if (typeof message === "string") return message;
  }
  return JSON.stringify(ev);
}

export function isRuntimeAssistantTextEvent(type: string): boolean {
  return (
    type === "assistant_response" ||
    type === "agent.message" ||
    type === "content_block_start" ||
    type === "content_block_delta" ||
    type === "message_delta"
  );
}

export function isRuntimeThinkingEvent(type: string): boolean {
  return type === "thinking_back" || type === "agent.thinking" || type === "agent.reasoning";
}

export function isRuntimeToolEvent(type: string): boolean {
  return (
    type === "tool_call" ||
    type === "tool_result" ||
    type === "agent.tool_use" ||
    type === "agent.tool_result"
  );
}

export function isRuntimeTurnStartEvent(type: string): boolean {
  return (
    type === "span.model_request_start" ||
    type === "session.status_running" ||
    type === "session.thread_status_running"
  );
}

function runtimeToolId(ev: RuntimeAgentEvent): string {
  const id = ev.tool_use_id ?? ev.id;
  return typeof id === "string" && id ? id : `tool_${Date.now().toString(36)}`;
}

function runtimeToolStatus(ev: RuntimeAgentEvent): string {
  if (typeof ev.status === "string") return ev.status;
  if (ev.type === "tool_result" || ev.type === "agent.tool_result") return "completed";
  if (ev.error) return "error";
  return "running";
}

function runtimeEventKey(ev: RuntimeAgentEvent): string {
  const id = ev.id;
  if (typeof id === "string" && id) return `id:${id}`;
  const type = typeof ev.type === "string" ? ev.type : "";
  const createdAt = ev.created_at ?? ev.timestamp ?? ev.time;
  if (createdAt) return `${type}:${String(createdAt)}:${runtimeEventText(ev)}`;
  return `${type}:${JSON.stringify(ev)}`;
}

function runtimeUserText(ev: RuntimeAgentEvent): string {
  return runtimeTextValue(ev.content ?? ev.text ?? ev.message).trim();
}

function isLocalRuntimeUserEvent(ev: RuntimeAgentEvent): boolean {
  return ev.type === "user.message" && ev.local === true;
}

export function mergeRuntimeEventList(
  current: RuntimeAgentEvent[],
  incoming: RuntimeAgentEvent | RuntimeAgentEvent[],
): RuntimeAgentEvent[] {
  const events = Array.isArray(incoming) ? incoming : [incoming];
  let next = current;
  const seen = new Set(current.map(runtimeEventKey));

  for (const ev of events) {
    const key = runtimeEventKey(ev);
    if (seen.has(key)) continue;

    if (ev.type === "user.message" && !isLocalRuntimeUserEvent(ev)) {
      const text = runtimeUserText(ev);
      if (text) {
        next = next.filter((candidate) => (
          !isLocalRuntimeUserEvent(candidate) || runtimeUserText(candidate) !== text
        ));
      }
    }

    next = [...next, ev];
    seen.add(key);
  }

  return next;
}

function makeTextMessage(sessionId: string, role: "user" | "assistant", id: string, text: string): HarnessMessage {
  return {
    info: { id, role, sessionID: sessionId },
    parts: [
      {
        id: `${id}_text`,
        messageID: id,
        sessionID: sessionId,
        type: "text",
        text,
      },
    ],
  };
}

export type QueuedPrompt = {
  id: string;
  text: string;
};

export function makeQueuedPromptMessage(sessionId: string, prompt: QueuedPrompt): HarnessMessage {
  return {
    ...makeTextMessage(sessionId, "user", prompt.id, prompt.text),
    info: { id: prompt.id, role: "user", sessionID: sessionId, status: "queued" },
  };
}

export function runtimeEventsToMessages(
  sessionId: string,
  events: RuntimeAgentEvent[],
  status: "idle" | "busy",
): HarnessMessage[] {
  const messages: HarnessMessage[] = [];
  let assistant: HarnessMessage | null = null;
  let turnIndex = 0;

  const ensureAssistant = (seed?: string): HarnessMessage => {
    if (assistant && !assistant.info.finish) return assistant;
    turnIndex += 1;
    const messageId = `${sessionId}_runtime_turn_${seed ?? turnIndex}`;
    assistant = {
      info: { id: messageId, role: "assistant", sessionID: sessionId },
      parts: [],
    };
    messages.push(assistant);
    return assistant;
  };

  const appendPartText = (message: HarnessMessage, kind: "text" | "thinking", text: string) => {
    if (!text) return;
    const partId = `${message.info.id}_${kind}`;
    const existing = message.parts.find((part) => part.id === partId);
    if (existing && "text" in existing) {
      existing.text = `${existing.text}${text}`;
      return;
    }
    message.parts.push({
      id: partId,
      messageID: message.info.id,
      sessionID: sessionId,
      type: kind,
      text,
    });
  };

  const upsertToolPart = (message: HarnessMessage, ev: RuntimeAgentEvent) => {
    const toolId = runtimeToolId(ev);
    const partId = `${message.info.id}_${toolId}`;
    const name = typeof ev.name === "string" ? ev.name : "tool";
    const statusValue = runtimeToolStatus(ev);
    const existing = message.parts.find((part) => part.id === partId && part.type === "tool");
    if (existing && existing.type === "tool") {
      existing.tool = existing.tool || name;
      existing.state = {
        ...existing.state,
        status: statusValue,
        input: existing.state.input ?? ev.input,
        output: ev.output ?? existing.state.output,
        error: ev.error ?? existing.state.error,
      };
      return;
    }
    message.parts.push({
      id: partId,
      messageID: message.info.id,
      sessionID: sessionId,
      type: "tool",
      tool: name,
      state: {
        status: statusValue,
        input: ev.input,
        output: ev.output,
        error: ev.error,
      },
    });
  };

  events.forEach((ev, index) => {
    const type = normalizedRuntimeEventType(ev);
    const seed = typeof ev.id === "string" && ev.id ? ev.id : String(index);

    if (type === "user.message") {
      const text = runtimeUserText(ev);
      if (text) {
        messages.push(makeTextMessage(sessionId, "user", `${sessionId}_user_${seed}`, text));
      }
      assistant = null;
      return;
    }

    if (type === "session.status_idle") {
      if (assistant) assistant.info.finish = "stop";
      assistant = null;
      return;
    }

    if (type === "session.status") {
      const eventStatus = ev.status;
      const statusType =
        typeof eventStatus === "string"
          ? eventStatus
          : eventStatus && typeof eventStatus === "object"
            ? (eventStatus as { type?: unknown }).type
            : undefined;
      if (statusType === "busy" || statusType === "running") {
        ensureAssistant(seed);
      }
      if (statusType === "idle" && assistant) {
        assistant.info.finish = "stop";
        assistant = null;
      }
      return;
    }

    if (isRuntimeTurnStartEvent(type)) {
      ensureAssistant(seed);
      return;
    }

    if (type === "session.error") {
      const message = ensureAssistant(seed);
      appendPartText(message, "text", `Error: ${runtimeErrorMessage(ev)}`);
      message.info.finish = "stop";
      return;
    }

    if (isRuntimeToolEvent(type)) {
      upsertToolPart(ensureAssistant(seed), ev);
      return;
    }

    if (!isRuntimeAssistantTextEvent(type) && !isRuntimeThinkingEvent(type)) return;
    const text = runtimeEventText(ev);
    if (!text && type !== "content_block_start") return;
    appendPartText(
      ensureAssistant(seed),
      isRuntimeThinkingEvent(type) ? "thinking" : runtimeEventPartKind(ev),
      text,
    );
  });

  if (status === "busy" && (messages.length === 0 || messages.at(-1)?.info.role === "user" || assistant === null)) {
    ensureAssistant("pending");
  }

  if (status === "idle") {
    const lastAssistant = messages.findLast((message) => message.info.role === "assistant" && !message.info.finish);
    if (lastAssistant) lastAssistant.info.finish = "stop";
  }
  return messages;
}

export function runtimeStatusFromEvents(events: RuntimeAgentEvent[]): "idle" | "busy" | null {
  let next: "idle" | "busy" | null = null;
  for (const ev of events) {
    const type = normalizedRuntimeEventType(ev);
    if (isLocalRuntimeUserEvent(ev)) {
      next = "busy";
      continue;
    }
    if (isRuntimeTurnStartEvent(type)) {
      next = "busy";
      continue;
    }
    if (type === "session.status_idle" || type === "session.thread_status_idle" || type === "session.error") {
      next = "idle";
      continue;
    }
    if (type === "session.status") {
      const status = ev.status;
      const statusType =
        typeof status === "string"
          ? status
          : status && typeof status === "object"
            ? (status as { type?: unknown }).type
            : undefined;
      if (statusType === "busy" || statusType === "running") next = "busy";
      if (statusType === "idle" || statusType === "error" || statusType === "failed") next = "idle";
    }
  }
  return next;
}

export function runtimeSessionStatusFromMetadata(status?: string, providerRunId?: unknown): "idle" | "busy" {
  if (status === "starting" || status === "running" || status === "busy") return "busy";
  if (status === "idle" || status === "error" || status === "completed" || status === "failed") return "idle";
  if (typeof providerRunId === "string" && providerRunId.trim()) return "busy";
  return "idle";
}
