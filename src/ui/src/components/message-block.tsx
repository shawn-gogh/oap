"use client";

import { useEffect, useMemo, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import {
  ChevronDown,
  Loader2,
  Send,
  Terminal,
  X,
} from "lucide-react";
import { CopyButton } from "@/components/copy-button";
import { MarkdownCodeBlock, MarkdownPre, HighlightedCode } from "@/components/code-block";
import { ToolErrorCard } from "@/components/tool-error-card";
import { TodoList, parseTodoItems, todoProgress } from "@/components/todo-list";
import { usePacedText } from "@/lib/hooks/use-paced-text";
import type { HarnessMessage, HarnessMessagePart } from "@/lib/types";

const markdownComponents = { code: MarkdownCodeBlock, pre: MarkdownPre };

// Adapter: derive the local-message shape LAP's components consume from our
// HarnessMessage (which carries info + parts). Sub-threads / permissions /
// attachments are not supported here.
interface LocalMessage {
  id: string;
  role: "user" | "assistant";
  text?: string;
  parts: HarnessMessagePart[];
  status?: "queued" | "in_progress" | "completed" | "failed";
  error?: string;
  latency_ms?: number;
  model?: string;
  harness?: string;
  tokens?: { input: number; output: number; total: number; cache?: { read: number; write: number } };
  cost?: number;
}

type RenderItem =
  | { type: "part"; part: HarnessMessagePart; key: string }
  | { type: "toolGroup"; parts: HarnessMessagePart[]; key: string };

type ToolPart = Extract<HarnessMessagePart, { type: "tool" }>;

/** Stable key from the part's runtime id — index-based keys shift when a
 * burst grows during the 2s poll merge, resetting expand state to the wrong
 * row. */
function partKey(part: HarnessMessagePart, fallback: string): string {
  const id = (part as { id?: unknown }).id;
  return typeof id === "string" && id ? id : fallback;
}

function toLocal(m: HarnessMessage): LocalMessage {
  const role = m.info.role;
  const parts = Array.isArray(m.parts) ? m.parts : [];
  const text = parts
    .filter((p): p is Extract<HarnessMessagePart, { type: "text" }> => p.type === "text")
    .map((p) => p.text)
    .join("\n");
  let status: LocalMessage["status"];
  let latency_ms: number | undefined;
  const infoStatus = (m.info as Record<string, unknown>).status;
  if (
    infoStatus === "queued" ||
    infoStatus === "in_progress" ||
    infoStatus === "completed" ||
    infoStatus === "failed"
  ) {
    status = infoStatus;
  }
  if (role === "assistant") {
    const finish = m.info.finish;
    if (status) {
      status = status;
    } else if (!finish) {
      status = "in_progress";
    } else if (finish === "stop" || finish === "end_turn") {
      status = "completed";
    } else {
      status = "completed";
    }
    const created = m.info.time?.created;
    const completed = m.info.time?.completed;
    if (typeof created === "number" && typeof completed === "number") {
      latency_ms = completed - created;
    }
  }
  const providerID = (m.info as Record<string, unknown>).providerID as string | undefined;
  const modelID = (m.info as Record<string, unknown>).modelID as string | undefined;
  const model = providerID && modelID ? `${providerID}/${modelID}` : modelID;
  const infoRecord = m.info as Record<string, unknown>;
  const harness = (infoRecord.agent ?? infoRecord.harness) as string | undefined;
  const tokens = (m.info as Record<string, unknown>).tokens as LocalMessage["tokens"] | undefined;
  const cost = (m.info as Record<string, unknown>).cost as number | undefined;

  return {
    id: (m.info.id as string | undefined) ?? "",
    role,
    text,
    parts,
    status,
    latency_ms,
    model,
    harness,
    tokens,
    cost,
  };
}

function InnerMessageBlock({
  msg,
  isFirstUser,
  onCancelQueued,
  onSendQueued,
  queuedActionBusy,
  hideTodoTools,
  showProgressIndicator,
}: {
  msg: LocalMessage;
  isFirstUser: boolean;
  onCancelQueued?: (msgId: string) => void;
  onSendQueued?: (msgId: string) => void;
  queuedActionBusy?: boolean;
  hideTodoTools: boolean;
  showProgressIndicator: boolean;
}) {
  if (msg.role === "user") {
    return (
      <UserPromptBlock
        id={msg.id}
        content={msg.text ?? ""}
        emphasized={isFirstUser}
        status={msg.status}
        onCancelQueued={onCancelQueued}
        onSendQueued={onSendQueued}
        queuedActionBusy={queuedActionBusy}
      />
    );
  }
  return (
    <AssistantBlock
      msg={msg}
      onCancelQueued={onCancelQueued}
      onSendQueued={onSendQueued}
      queuedActionBusy={queuedActionBusy}
      hideTodoTools={hideTodoTools}
      showProgressIndicator={showProgressIndicator}
    />
  );
}

function UserPromptBlock({
  id,
  content,
  emphasized,
  status,
  onCancelQueued,
  onSendQueued,
  queuedActionBusy,
}: {
  id: string;
  content: string;
  emphasized: boolean;
  status?: LocalMessage["status"];
  onCancelQueued?: (msgId: string) => void;
  onSendQueued?: (msgId: string) => void;
  queuedActionBusy?: boolean;
}) {
  const queued = status === "queued";
  return (
    <div className="flex justify-end">
      <div className="flex max-w-[min(740px,82%)] flex-col items-end gap-1.5">
        <div
          className={`w-full rounded-[18px] border border-border/80 bg-muted/65 px-5 py-3 text-[15px] leading-relaxed text-foreground shadow-[0_1px_2px_rgba(15,23,42,0.04)] dark:bg-muted/45 ${
            emphasized ? "ring-1 ring-ring/30" : ""
          } ${queued ? "opacity-75" : ""}`}
        >
          {content && <div className="whitespace-pre-wrap">{content}</div>}
        </div>
        {queued && (
          <div className="flex items-center gap-1.5 pr-1 text-xs text-muted-foreground">
            <span aria-hidden className="size-1.5 rounded-full bg-muted-foreground/40" />
            queued
            {onSendQueued && (
              <button
                type="button"
                onClick={() => onSendQueued(id)}
                disabled={queuedActionBusy}
                title="Interrupt active run and send queued message"
                className="inline-flex items-center gap-1 rounded px-1.5 py-0.5 font-medium text-foreground transition-colors hover:bg-muted disabled:pointer-events-none disabled:opacity-50"
                aria-label="Interrupt active run and send queued message"
              >
                {queuedActionBusy ? (
                  <Loader2 className="size-3 animate-spin motion-reduce:animate-none" />
                ) : (
                  <Send className="size-3" />
                )}
                <span>Interrupt and send</span>
              </button>
            )}
            {onCancelQueued && (
              <button
                type="button"
                onClick={() => onCancelQueued(id)}
                disabled={queuedActionBusy}
                title="Cancel queued message"
                className="rounded p-0.5 transition-colors hover:bg-muted hover:text-foreground disabled:pointer-events-none disabled:opacity-50"
                aria-label="Cancel queued message"
              >
                <X className="size-3" />
              </button>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function AssistantBlock({
  msg,
  onCancelQueued,
  onSendQueued,
  queuedActionBusy,
  hideTodoTools,
  showProgressIndicator,
}: {
  msg: LocalMessage;
  onCancelQueued?: (msgId: string) => void;
  onSendQueued?: (msgId: string) => void;
  queuedActionBusy?: boolean;
  hideTodoTools: boolean;
  showProgressIndicator: boolean;
}) {
  const failed = msg.status === "failed";
  const inProgress = msg.status === "in_progress";
  const queued = msg.status === "queued";
  const parts = msg.parts ?? [];

  const visibleParts = parts.filter((p) => {
    const t = typeof p?.type === "string" ? (p.type as string) : "";
    if (p.type === "tool" && hideTodoTools && isTodoTool(p.tool)) return false;
    return (
      t === "text" ||
      t === "reasoning" ||
      t === "thinking" ||
      t === "tool" ||
      t === "image"
    );
  });
  const renderItems = groupRenderItems(visibleParts);
  const hasRunningTool = visibleParts.some(
    (part) => part.type === "tool" && (part.state.status === "running" || part.state.status === "pending"),
  );
  const details = messageDetails(msg);
  // An in-progress turn always renders at least a waiting line — returning
  // null here left blank gaps that read as "the session stopped responding".
  if (!failed && !queued && !inProgress && visibleParts.length === 0) return null;

  return (
    <article className="group/turn flex flex-col gap-3 py-1">
      {failed && msg.text ? (
        <div
          className="sessions-md max-w-[920px] text-[15px] leading-7 text-red-600 dark:text-red-400"
        >
          <ReactMarkdown remarkPlugins={[remarkGfm]} components={markdownComponents}>{msg.text}</ReactMarkdown>
        </div>
      ) : queued ? (
        <div className="flex items-center gap-2 text-[13px] text-muted-foreground leading-relaxed">
          <span aria-hidden className="size-1.5 rounded-full bg-muted-foreground/40" />
          queued
          {onSendQueued && (
            <button
              type="button"
              onClick={() => onSendQueued(msg.id)}
              disabled={queuedActionBusy}
              title="Interrupt active run and send queued message"
              className="ml-1 inline-flex items-center gap-1 rounded px-1.5 py-0.5 font-medium text-foreground transition-colors hover:bg-muted disabled:pointer-events-none disabled:opacity-50"
              aria-label="Interrupt active run and send queued message"
            >
              {queuedActionBusy ? (
                <Loader2 className="size-3 animate-spin motion-reduce:animate-none" />
              ) : (
                <Send className="size-3" />
              )}
              <span>Interrupt and send</span>
            </button>
          )}
          {onCancelQueued && (
            <button
              type="button"
              onClick={() => onCancelQueued(msg.id)}
              disabled={queuedActionBusy}
              title="Cancel queued message"
              className="ml-1 p-0.5 rounded hover:bg-muted hover:text-foreground transition-colors disabled:pointer-events-none disabled:opacity-50"
              aria-label="Cancel queued message"
            >
              <X className="w-3 h-3" />
            </button>
          )}
        </div>
      ) : inProgress && visibleParts.length === 0 ? (
        msg.text ? (
          <div className="sessions-md max-w-[920px] text-[15px] leading-7 text-foreground">
            <ReactMarkdown remarkPlugins={[remarkGfm]} components={markdownComponents}>{msg.text}</ReactMarkdown>
          </div>
        ) : (
          <div className="flex items-center gap-2 text-sm text-muted-foreground leading-relaxed">
            <Loader2 className="w-3 h-3 animate-spin motion-reduce:animate-none" />
            正在等待模型响应
          </div>
        )
      ) : (
        <>
          {renderItems.map((item, index) =>
            item.type === "toolGroup" ? (
              <ToolCluster key={item.key} parts={item.parts} />
            ) : (
              <PartBlock
                key={item.key}
                part={item.part}
                streaming={inProgress && index === renderItems.length - 1}
              />
            ),
          )}
          {inProgress && showProgressIndicator && !hasRunningTool && (
            <div className="flex items-center gap-2 pt-1 text-[13px] text-muted-foreground">
              <Loader2 className="size-3.5 animate-spin motion-reduce:animate-none" />
              <span>正在等待模型响应</span>
            </div>
          )}
        </>
      )}

      {failed && msg.error && (
        <div className="mono text-[11px] text-red-600 dark:text-red-400">{msg.error}</div>
      )}

      {!inProgress && !failed && (
        <div className="mono flex flex-wrap items-center gap-x-2.5 gap-y-1 text-[11px] text-muted-foreground/75 transition-colors group-hover/turn:text-muted-foreground">
          {msg.harness && (
            <span className={`rounded-md px-1.5 py-0.5 text-[11px] font-mono font-medium ${
              msg.harness === "github-copilot"
                ? "bg-sky-500/15 text-sky-600 dark:text-sky-400"
                : msg.harness === "claude-code"
                  ? "bg-orange-500/15 text-orange-600 dark:text-orange-400"
                  : "bg-muted text-muted-foreground"
            }`}>
              {msg.harness}
            </span>
          )}
          {details.map((detail) => (
            <span key={detail}>{detail}</span>
          ))}
        </div>
      )}
    </article>
  );
}

function groupRenderItems(parts: HarnessMessagePart[]): RenderItem[] {
  const items: RenderItem[] = [];
  let toolRun: HarnessMessagePart[] = [];

  const flushTools = () => {
    if (toolRun.length === 0) return;
    items.push({
      type: "toolGroup",
      parts: toolRun,
      key: partKey(toolRun[0], `tools-${items.length}`),
    });
    toolRun = [];
  };

  parts.forEach((part, index) => {
    const t = typeof part?.type === "string" ? part.type : "";
    if (t === "tool") {
      toolRun.push(part);
      return;
    }
    flushTools();
    items.push({ type: "part", part, key: partKey(part, `${t || "part"}-${index}`) });
  });
  flushTools();

  return items;
}

function messageDetails(msg: LocalMessage): string[] {
  const details: string[] = [];
  if (msg.model) details.push(msg.model);
  if (typeof msg.latency_ms === "number") details.push(formatLatency(msg.latency_ms));
  if (msg.tokens) {
    const tokenText = `↑${msg.tokens.input.toLocaleString()} ↓${msg.tokens.output.toLocaleString()}`;
    const cacheText = msg.tokens.cache && msg.tokens.cache.read > 0
      ? ` cache ${msg.tokens.cache.read.toLocaleString()}`
      : "";
    details.push(tokenText + cacheText);
  }
  if (typeof msg.cost === "number" && msg.cost > 0) details.push(`$${msg.cost.toFixed(4)}`);
  return details;
}

function formatLatency(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

function PartBlock({ part, streaming = false }: { part: HarnessMessagePart; streaming?: boolean }) {
  const t = typeof part?.type === "string" ? part.type : "";
  if (t === "text") {
    const text = typeof (part as { text?: unknown }).text === "string" ? (part as { text: string }).text : "";
    if (!text) return null;
    return <TextPart text={text} streaming={streaming} />;
  }
  if (t === "reasoning" || t === "thinking") {
    const text = typeof (part as { text?: unknown }).text === "string" ? (part as { text: string }).text : "";
    if (!text) return null;
    return <ReasoningBlock text={text} />;
  }
  if (t === "tool") {
    return <ToolBlock part={part} />;
  }
  return null;
}

function TextPart({ text, streaming }: { text: string; streaming: boolean }) {
  const shown = usePacedText(text, streaming);
  return (
    <div className="group/text relative max-w-[920px]">
      <div className="sessions-md text-[15px] leading-7 text-foreground">
        <ReactMarkdown remarkPlugins={[remarkGfm]} components={markdownComponents}>{shown}</ReactMarkdown>
      </div>
      <CopyButton
        text={text}
        className="absolute right-0 top-0 opacity-0 transition-opacity group-hover/text:opacity-100"
      />
    </div>
  );
}

function ReasoningBlock({ text }: { text: string }) {
  const [open, setOpen] = useState(false);
  const preview = text.length > 360 ? text.slice(0, 360) + "…" : text;
  return (
    <div className="max-w-[920px] border-l-2 border-border pl-3 text-[13px] text-muted-foreground italic leading-relaxed">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        aria-label={open ? "Collapse reasoning" : "Expand reasoning"}
        aria-expanded={open}
        className="flex items-start gap-1 text-left hover:text-foreground"
      >
        <ChevronDown
          className={`w-3 h-3 mt-1 shrink-0 transition-transform ${
            open ? "" : "-rotate-90"
          }`}
        />
        <span className="whitespace-pre-wrap">{open ? text : preview}</span>
      </button>
    </div>
  );
}

export function isTodoTool(tool: string): boolean {
  return /todo/i.test(tool);
}

export function toolDescriptor(tool: string, input: unknown): string {
  const o = (input && typeof input === "object" ? input : {}) as Record<
    string,
    unknown
  >;
  const pick = (...keys: string[]): string => {
    for (const k of keys) {
      const v = o[k];
      if (typeof v === "string" && v) return v;
    }
    return "";
  };
  const n = tool.toLowerCase();
  if (isTodoTool(n)) {
    const items = parseTodoItems(input);
    if (items) {
      const { done, total } = todoProgress(items);
      return `${done}/${total} done`;
    }
    return "";
  }
  if (n === "task") return pick("description");
  if (n === "bash") return pick("command", "description");
  if (n.includes("gmail")) return pick("subject", "to", "thread_id", "message_id");
  if (n.includes("pylon") || n.includes("linear")) return pick("issue_id", "title", "state");
  if (n.includes("read") || n.includes("edit") || n.includes("write") || n.includes("patch"))
    return pick("filePath", "file_path", "path");
  if (n.includes("grep") || n.includes("glob") || n.includes("find"))
    return pick("pattern", "query");
  return "";
}

export function toolLabel(tool: string): string {
  return tool
    .replace(/^mcp__/i, "")
    .replace(/^functions\s+/i, "")
    .replace(/^mcp\s+/i, "")
    .replace(/[_-]+/g, " ")
    .replace(/\s+/g, " ")
    .trim()
    .replace(/\b\w/g, (c) => c.toUpperCase());
}

function isFailedToolStatus(status: string): boolean {
  return status === "error" || status === "timed_out" || status === "aborted";
}

function toolPartStatus(part: ToolPart): string {
  const status = part.state?.status;
  return typeof status === "string" ? status : "running";
}

function basename(path: string): string {
  const index = path.lastIndexOf("/");
  return index >= 0 ? path.slice(index + 1) : path;
}

/** Codex-style work burst: a collapsed summary line ("已编辑 N 个文件 · …"),
 * a live activity line while a tool is running, and — on expand — a flat
 * list of compact one-line entries. */
function ToolCluster({ parts }: { parts: HarnessMessagePart[] }) {
  const [open, setOpen] = useState(false);
  const toolParts = parts.filter((p): p is ToolPart => p.type === "tool");
  const summary = toolActivitySummary(parts);
  const runningPart = toolParts.find((part) => {
    const status = toolPartStatus(part);
    return status === "running" || status === "pending";
  });
  const failedCount = toolParts.filter((part) => isFailedToolStatus(toolPartStatus(part))).length;

  const startedAt =
    typeof runningPart?.state?.startedAt === "number" ? runningPart.state.startedAt : undefined;
  const [now, setNow] = useState(0);
  useEffect(() => {
    if (!runningPart || startedAt === undefined) return;
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [runningPart, startedAt]);
  const runningDuration =
    startedAt !== undefined && now > 0 ? formatToolDuration(Math.max(0, now - startedAt)) : "";

  return (
    <div className="max-w-[920px] py-0.5 text-[13px]">
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        aria-expanded={open}
        className="flex max-w-full items-center gap-2 text-left text-muted-foreground transition-colors hover:text-foreground"
      >
        {runningPart ? (
          <Loader2 className="size-3.5 shrink-0 animate-spin motion-reduce:animate-none" />
        ) : (
          <Terminal className="size-3.5 shrink-0" />
        )}
        <span className="truncate">{summary}</span>
        {failedCount > 0 && (
          <span className="shrink-0 text-red-600 dark:text-red-400">{failedCount} 个失败</span>
        )}
        <ChevronDown className={`size-3.5 shrink-0 transition-transform ${open ? "rotate-180" : ""}`} />
      </button>
      {runningPart && !open && (
        <div className="mt-1 pl-[22px] text-[13px] text-muted-foreground/80">
          {liveActivityLabel(runningPart)}
          {runningDuration && `，已持续 ${runningDuration}`}
        </div>
      )}
      {open && (
        <div className="mt-1 flex flex-col pl-[22px]">
          {toolParts.map((part, index) => (
            <ToolBlock key={partKey(part, `tool-${index}`)} part={part} />
          ))}
        </div>
      )}
    </div>
  );
}

function liveActivityLabel(part: ToolPart): string {
  const n = part.tool.toLowerCase();
  const desc = toolDescriptor(part.tool, part.state?.input);
  if (n === "bash" || n.includes("shell") || n.includes("exec")) return "正在运行命令";
  if (n.includes("edit") || n.includes("write") || n.includes("patch"))
    return `正在编辑 ${desc ? basename(desc) : "文件"}`;
  if (n.includes("read")) return `正在读取 ${desc ? basename(desc) : "文件"}`;
  if (n.includes("grep") || n.includes("search") || n.includes("find") || n.includes("glob"))
    return "正在搜索代码";
  return `正在运行 ${toolLabel(part.tool)}`;
}

function toolActivitySummary(parts: HarnessMessagePart[]): string {
  const tools = parts.filter((part): part is Extract<HarnessMessagePart, { type: "tool" }> => part.type === "tool");
  const counts = { read: 0, edit: 0, command: 0, search: 0, other: 0 };
  for (const part of tools) {
    const name = part.tool.toLowerCase();
    if (name === "bash" || name.includes("shell") || name.includes("exec")) counts.command += 1;
    else if (name.includes("edit") || name.includes("write") || name.includes("patch")) counts.edit += 1;
    else if (name.includes("grep") || name.includes("search") || name.includes("find") || name.includes("glob")) counts.search += 1;
    else if (name.includes("read") || name.includes("list") || name.endsWith("ls")) counts.read += 1;
    else counts.other += 1;
  }
  const labels = [
    counts.edit ? `已编辑 ${counts.edit} 个文件` : "",
    counts.read ? `已读取 ${counts.read} 个文件` : "",
    counts.search ? `已搜索 ${counts.search} 次` : "",
    counts.command ? `已运行 ${counts.command} 条命令` : "",
    counts.other ? `已调用 ${counts.other} 个工具` : "",
  ].filter(Boolean);
  return labels.join(" · ") || "运行活动";
}

/** Line delta for edit/write style tools, computed from the tool input —
 * the runtime provides no numstat. */
export function diffStat(tool: string, input: unknown): { added: number; removed: number } | null {
  const n = tool.toLowerCase();
  if (!n.includes("edit") && !n.includes("write") && !n.includes("patch")) return null;
  const oldText = inputString(input, "oldString", "old_string");
  const newText = inputString(input, "newString", "new_string", "content", "text");
  if (!oldText && !newText) return null;
  const lines = (text: string) => (text ? text.split("\n").length : 0);
  return { added: lines(newText), removed: lines(oldText) };
}

/** Compact one-line tool entry (Codex style): verbed title, filename in
 * accent color, +N -N for edits; details expand on click. */
export function ToolBlock({ part }: { part: HarnessMessagePart }) {
  const p = part as ToolPart;
  const toolName = typeof p.tool === "string" ? p.tool : "tool";
  const state = (p.state as Record<string, unknown> | undefined) ?? {};
  const status = typeof state.status === "string" ? state.status : "running";
  const failedStatus = isFailedToolStatus(status);
  const [open, setOpen] = useState(failedStatus);
  // A tool that fails after mount pops its details open so the error is seen.
  useEffect(() => {
    if (failedStatus) setOpen(true);
  }, [failedStatus]);
  const input = state.input;
  const output = state.output;
  const errorOut = state.error;
  const errorMessage = typeof state.errorMessage === "string" ? state.errorMessage : "";
  const startedAt = typeof state.startedAt === "number" ? state.startedAt : undefined;
  const running = status === "running" || status === "pending";
  const [now, setNow] = useState(0);
  useEffect(() => {
    if (!running || startedAt === undefined) return;
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [running, startedAt]);
  const duration = useMemo(() => {
    if (!running || startedAt === undefined || now === 0) return "";
    return formatToolDuration(Math.max(0, now - startedAt));
  }, [now, running, startedAt]);

  const desc = toolDescriptor(toolName, input);
  const todoItems = isTodoTool(toolName) ? parseTodoItems(input, output) : null;
  const hasDetails = input !== undefined || output !== undefined || errorOut !== undefined;
  const n = toolName.toLowerCase();
  const stat = diffStat(toolName, input);
  const isEdit = n.includes("edit") || n.includes("write") || n.includes("patch");
  const isCommand = n === "bash" || n.includes("shell") || n.includes("exec");
  const isSearch =
    n.includes("grep") || n.includes("search") || n.includes("find") || n.includes("glob");
  const isRead = n.includes("read") || n.includes("list") || n.endsWith("ls");
  const statusLabel =
    status === "timed_out" ? "已超时" : status === "aborted" ? "已中断" : "失败";

  return (
    <div className="max-w-[920px] text-[13px]">
      <button
        type="button"
        onClick={() => hasDetails && setOpen((v) => !v)}
        aria-expanded={hasDetails ? open : undefined}
        className={`flex max-w-full min-w-0 items-center gap-1.5 rounded px-1 py-0.5 text-left leading-6 text-muted-foreground ${
          hasDetails ? "cursor-pointer transition-colors hover:bg-muted/40 hover:text-foreground" : "cursor-default"
        }`}
      >
        {todoItems ? (
          <span className="min-w-0 truncate">Todos {desc && <span className="mono">{desc}</span>}</span>
        ) : isEdit && desc ? (
          <span className="min-w-0 truncate">
            已编辑{" "}
            <span className="text-primary" title={desc}>
              {basename(desc)}
            </span>
            {stat && (
              <>
                {" "}
                <span className="text-emerald-600 dark:text-emerald-400">+{stat.added}</span>{" "}
                <span className="text-red-600 dark:text-red-400">-{stat.removed}</span>
              </>
            )}
          </span>
        ) : isCommand ? (
          <span className="min-w-0 truncate">
            {running ? "正在运行" : "已运行"}{" "}
            <span className="mono text-muted-foreground/80" title={desc}>
              {desc || toolLabel(toolName)}
            </span>
          </span>
        ) : isSearch ? (
          <span className="min-w-0 truncate">
            Searched for <span className="mono text-muted-foreground/80">{desc}</span>
          </span>
        ) : isRead && desc ? (
          <span className="min-w-0 truncate">
            Read{" "}
            <span title={desc} className="text-foreground/80">
              {basename(desc)}
            </span>
          </span>
        ) : (
          <span className="min-w-0 truncate">
            {toolLabel(toolName)}
            {desc && <span className="mono ml-1.5 text-muted-foreground/80">{desc}</span>}
          </span>
        )}
        {running && duration && (
          <span className="shrink-0 text-amber-600 dark:text-amber-400">已持续 {duration}</span>
        )}
        {failedStatus && (
          <span className="shrink-0 text-red-600 dark:text-red-400">{statusLabel}</span>
        )}
        {hasDetails && (
          <ChevronDown
            className={`size-3 shrink-0 text-muted-foreground/60 transition-transform ${
              open ? "rotate-180" : ""
            }`}
          />
        )}
      </button>

      {open && hasDetails && (
        <div className="ml-4 mt-1 mb-1.5 flex flex-col gap-2 rounded-lg border border-border/70 bg-muted/20 p-3">
          {todoItems ? (
            <TodoList items={todoItems} />
          ) : (
            <RichToolDetails tool={toolName} input={input} output={output} />
          )}
          {(errorMessage || errorOut !== undefined) && <ToolErrorCard error={errorMessage || errorOut} />}
        </div>
      )}
    </div>
  );
}

function formatToolDuration(milliseconds: number): string {
  const seconds = Math.floor(milliseconds / 1000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  return `${minutes}m ${String(seconds % 60).padStart(2, "0")}s`;
}

function inputString(input: unknown, ...keys: string[]): string {
  if (!input || typeof input !== "object") return "";
  const record = input as Record<string, unknown>;
  for (const key of keys) {
    const value = record[key];
    if (typeof value === "string" && value) return value;
  }
  return "";
}

function outputText(output: unknown): string {
  const clean = (value: string) => value.replace(/\n*<shell_metadata>[\s\S]*?<\/shell_metadata>\s*$/i, "").trimEnd();
  if (typeof output === "string") return clean(output);
  if (output && typeof output === "object") {
    const record = output as Record<string, unknown>;
    if (typeof record.output === "string") return clean(record.output);
    if (typeof record.stdout === "string") return clean(record.stdout);
    if (typeof record.text === "string") return clean(record.text);
  }
  return "";
}

function langForFile(path: string): string {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  const map: Record<string, string> = {
    ts: "typescript", tsx: "tsx", js: "javascript", jsx: "jsx", rs: "rust",
    py: "python", go: "go", json: "json", yaml: "yaml", yml: "yaml",
    md: "markdown", sh: "bash", sql: "sql", css: "css", html: "html", toml: "toml",
  };
  return map[ext] ?? "text";
}

/** Two-block diff (removed lines, then added lines) — no line matching, but
 * far more readable than raw JSON for edit-style tool calls. */
function DiffView({ oldText, newText }: { oldText: string; newText: string }) {
  return (
    <div className="mono overflow-x-auto rounded-md border border-border text-xs leading-relaxed">
      {oldText && oldText.split("\n").map((line, i) => (
        <div key={`d${i}`} className="whitespace-pre bg-red-500/10 px-2 text-red-700 dark:text-red-400">
          <span className="select-none pr-2 opacity-60">-</span>{line}
        </div>
      ))}
      {newText && newText.split("\n").map((line, i) => (
        <div key={`a${i}`} className="whitespace-pre bg-emerald-500/10 px-2 text-emerald-700 dark:text-emerald-400">
          <span className="select-none pr-2 opacity-60">+</span>{line}
        </div>
      ))}
    </div>
  );
}

/** Structured rendering for common tools (bash → terminal block, edit → diff,
 * write → highlighted file content); falls back to raw input/output. */
function RichToolDetails({ tool, input, output }: { tool: string; input: unknown; output: unknown }) {
  const n = tool.toLowerCase();

  if (n === "bash" || n.endsWith("bash")) {
    const command = inputString(input, "command");
    const out = outputText(output);
    const exit = input && typeof output === "object" && output
      ? (output as Record<string, unknown>).exit_code ?? (output as Record<string, unknown>).exitCode
      : undefined;
    if (command || out) {
      return (
        <div className="flex flex-col gap-2">
          {command && <HighlightedCode code={`$ ${command}`} lang="bash" />}
          {out && (
            <pre className="mono max-h-64 overflow-auto whitespace-pre-wrap rounded-md border border-border bg-background px-2 py-1.5 text-xs leading-relaxed text-muted-foreground">{out}</pre>
          )}
          {typeof exit === "number" && exit !== 0 && (
            <span className="mono text-[11px] text-red-600 dark:text-red-400">exit {exit}</span>
          )}
        </div>
      );
    }
  }

  if (n.includes("edit") || n.includes("patch")) {
    const oldText = inputString(input, "oldString", "old_string");
    const newText = inputString(input, "newString", "new_string");
    const path = inputString(input, "filePath", "file_path", "path");
    if (oldText || newText) {
      return (
        <div className="flex flex-col gap-1.5">
          {path && <span className="mono text-[11px] text-muted-foreground">{path}</span>}
          <DiffView oldText={oldText} newText={newText} />
        </div>
      );
    }
  }

  if (n.includes("write")) {
    const content = inputString(input, "content", "text");
    const path = inputString(input, "filePath", "file_path", "path");
    if (content) {
      return (
        <div className="flex flex-col gap-1.5">
          {path && <span className="mono text-[11px] text-muted-foreground">{path}</span>}
          <div className="max-h-64 overflow-auto">
            <HighlightedCode code={content} lang={langForFile(path)} />
          </div>
        </div>
      );
    }
  }

  return (
    <>
      {input !== undefined && <ToolKv label="input" value={input} />}
      {output !== undefined && <ToolKv label="output" value={output} />}
    </>
  );
}

function ToolKv({ label, value }: { label: string; value: unknown }) {
  const isString = typeof value === "string";
  const text = isString ? (value as string) : JSON.stringify(value, null, 2);
  return (
    <div className="flex flex-col gap-1">
      <span className="mono text-[11px] uppercase tracking-wide text-muted-foreground">
        {label}
      </span>
      <HighlightedCode code={text} lang={isString ? "text" : "json"} />
    </div>
  );
}

export function MessageBlock({
  msg,
  onCancelQueued,
  onSendQueued,
  queuedActionBusy,
  hideTodoTools = false,
  showProgressIndicator = true,
}: {
  msg: HarnessMessage;
  onCancelQueued?: (msgId: string) => void;
  onSendQueued?: (msgId: string) => void;
  queuedActionBusy?: boolean;
  hideTodoTools?: boolean;
  showProgressIndicator?: boolean;
}) {
  const local = toLocal(msg);
  return (
    <InnerMessageBlock
      msg={local}
      isFirstUser={false}
      onCancelQueued={onCancelQueued}
      onSendQueued={onSendQueued}
      queuedActionBusy={queuedActionBusy}
      hideTodoTools={hideTodoTools}
      showProgressIndicator={showProgressIndicator}
    />
  );
}
