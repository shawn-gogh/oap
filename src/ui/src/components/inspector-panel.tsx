"use client";

import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type PointerEvent,
} from "react";
import {
  Activity,
  ChevronRight,
  Radio,
  Trash2,
  X,
  FileText,
  KeyRound,
  Wrench,
  Sparkles,
  Clock,
  Layers,
  CheckCircle2,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { runtimeEventSourceUrl } from "@/lib/api";
import { cn } from "@/lib/utils";

const DEFAULT_PANEL_WIDTH = 440;
const MIN_PANEL_WIDTH = 340;
const MAX_PANEL_WIDTH = 760;

interface OcEvent {
  type: string;
  [key: string]: unknown;
}

export interface Frame {
  ts: number;
  ev: OcEvent;
}

type InspectorTab = "timeline" | "files" | "context";

function summarize(ev: OcEvent): string {
  const text =
    typeof ev.text === "string"
      ? ev.text
      : typeof ev.delta === "string"
        ? ev.delta
        : "";
  switch (ev.type) {
    case "session.status_idle":
      return "智能体已完成当前步骤控制权返回";
    case "session.error":
      return String(
        typeof ev.error === "object" && ev.error
          ? ((ev.error as { message?: unknown }).message ?? "运行时执行异常")
          : (ev.error ?? "运行时执行异常"),
      );
    case "assistant_response":
    case "thinking_back":
    case "agent.message":
    case "agent.thinking":
    case "agent.reasoning":
      return text ? text.replace(/\s+/g, " ").slice(0, 110) : "接收到消息负载";
    case "session.status": {
      const status = ev.status as { type?: string } | string | undefined;
      return typeof status === "string" ? status : (status?.type ?? "会话状态更新");
    }
    default:
      return JSON.stringify(ev).slice(0, 110);
  }
}

function eventTone(type: string): { dot: string; text: string; bg: string } {
  if (type === "session.error")
    return { dot: "bg-destructive animate-pulse", text: "text-destructive", bg: "bg-destructive/10 border-destructive/30" };
  if (type === "session.status_idle")
    return { dot: "bg-emerald-500", text: "text-emerald-600 dark:text-emerald-400", bg: "bg-emerald-500/10 border-emerald-500/20" };
  if (type.includes("tool") || type.includes("action"))
    return { dot: "bg-blue-500", text: "text-blue-600 dark:text-blue-400", bg: "bg-blue-500/10 border-blue-500/20" };
  if (type.includes("thinking") || type.includes("reasoning"))
    return { dot: "bg-amber-500", text: "text-amber-600 dark:text-amber-400", bg: "bg-amber-500/10 border-amber-500/20" };
  return { dot: "bg-muted-foreground", text: "text-muted-foreground", bg: "bg-muted/40 border-border/60" };
}

function fmtTime(ts: number): string {
  return new Date(ts).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

function TimelineEventNode({ frame, isLast }: { frame: Frame; isLast: boolean }) {
  const [open, setOpen] = useState(false);
  const tone = eventTone(frame.ev.type);

  return (
    <div className="relative pl-6 pb-4">
      {/* Connecting Timeline Line */}
      {!isLast && (
        <span className="absolute left-2.5 top-3 -bottom-1 w-px bg-border/80" />
      )}
      {/* Node Bullet Dot */}
      <span className={cn("absolute left-1.5 top-2.5 size-2 rounded-full ring-4 ring-background", tone.dot)} />

      <div className="rounded-xl border border-border/70 bg-card/90 shadow-2xs overflow-hidden transition-all hover:border-border">
        <button
          type="button"
          onClick={() => setOpen((value) => !value)}
          aria-expanded={open}
          className="flex w-full items-start justify-between gap-2 p-3 text-left transition-colors hover:bg-muted/30"
        >
          <div className="min-w-0 flex-1 space-y-1">
            <div className="flex items-center gap-2">
              <span className={cn("font-mono text-[11px] font-bold tracking-tight", tone.text)}>
                {frame.ev.type}
              </span>
              <span className="ml-auto shrink-0 font-mono text-[10px] text-muted-foreground">
                {fmtTime(frame.ts)}
              </span>
            </div>
            <p className="text-xs text-muted-foreground leading-relaxed font-mono truncate">
              {summarize(frame.ev)}
            </p>
          </div>
          <ChevronRight className={cn("size-3.5 shrink-0 text-muted-foreground transition-transform mt-0.5", open && "rotate-90")} />
        </button>
        {open && (
          <div className="border-t border-border/60 bg-muted/20 p-3">
            <pre className="max-h-60 overflow-auto rounded-lg border border-border/80 bg-background p-3 font-mono text-[11px] leading-relaxed text-foreground whitespace-pre-wrap break-words selection:bg-blue-500/20">
              {JSON.stringify(frame.ev, null, 2)}
            </pre>
          </div>
        )}
      </div>
    </div>
  );
}

export function InspectorPanel({
  open,
  onClose,
  sessionId,
  initialFrames = [],
}: {
  open: boolean;
  onClose: () => void;
  sessionId: string;
  initialFrames?: Frame[];
}) {
  const [frames, setFrames] = useState<Frame[]>([]);
  const [activeTab, setActiveTab] = useState<InspectorTab>("timeline");
  const [hideHeartbeat, setHideHeartbeat] = useState(true);
  const [panelWidth, setPanelWidth] = useState(DEFAULT_PANEL_WIDTH);
  const scrollRef = useRef<HTMLDivElement | null>(null);

  const resizePanel = useCallback((width: number) => {
    setPanelWidth(Math.min(MAX_PANEL_WIDTH, Math.max(MIN_PANEL_WIDTH, width)));
  }, []);

  const startPanelResize = (event: PointerEvent<HTMLDivElement>) => {
    if (window.innerWidth < 1280) return;
    event.preventDefault();
    const startX = event.clientX;
    const startWidth = panelWidth;
    const move = (moveEvent: globalThis.PointerEvent) => {
      resizePanel(startWidth + startX - moveEvent.clientX);
    };
    const stop = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
      window.removeEventListener("pointercancel", stop);
      document.body.style.removeProperty("cursor");
      document.body.style.removeProperty("user-select");
    };
    document.body.style.setProperty("cursor", "col-resize");
    document.body.style.setProperty("user-select", "none");
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop);
    window.addEventListener("pointercancel", stop);
  };

  useEffect(() => {
    if (!open) return;
    setFrames(initialFrames.slice(-500));
    let source: EventSource | null = null;
    try {
      source = new EventSource(runtimeEventSourceUrl(sessionId));
    } catch {
      return;
    }
    source.onmessage = (message) => {
      try {
        const event = JSON.parse(message.data) as OcEvent;
        setFrames((current) => [...current.slice(-999), { ts: Date.now(), ev: event }]);
      } catch {
        return;
      }
    };
    return () => source?.close();
  }, [open, sessionId]);

  const shown = useMemo(
    () => (hideHeartbeat ? frames.filter((frame) => frame.ev.type !== "server.heartbeat") : frames),
    [frames, hideHeartbeat],
  );

  useEffect(() => {
    const element = scrollRef.current;
    if (element) element.scrollTop = element.scrollHeight;
  }, [shown]);

  if (!open) return null;

  return (
    <aside
      className="fixed inset-y-0 right-0 z-40 flex w-[min(440px,calc(100vw-1rem))] min-w-0 flex-col border-l border-border/80 bg-background shadow-xl xl:relative xl:inset-auto xl:z-auto xl:h-screen xl:w-[var(--inspector-panel-width)] xl:shrink-0 xl:shadow-none selection:bg-blue-500/20"
      style={{ "--inspector-panel-width": `${panelWidth}px` } as CSSProperties}
    >
      {/* Resize Handle */}
      <div
        role="separator"
        aria-label="调整检查器宽度"
        tabIndex={0}
        onPointerDown={startPanelResize}
        onDoubleClick={() => setPanelWidth(DEFAULT_PANEL_WIDTH)}
        className="group absolute inset-y-0 -left-1 z-50 hidden w-2 cursor-col-resize touch-none xl:block focus-visible:outline-none"
        title="拖动调整宽度，双击复位"
      >
        <span className="mx-auto block h-full w-px bg-transparent transition-colors group-hover:bg-blue-500 group-focus-visible:bg-blue-500" />
      </div>

      {/* Header */}
      <header className="flex h-12 shrink-0 items-center justify-between border-b border-border/80 px-4 bg-background/80 backdrop-blur">
        <div className="flex items-center gap-2">
          <div className="flex size-7 items-center justify-center rounded-lg bg-blue-500/10 text-blue-600 dark:text-blue-400 border border-blue-500/20">
            <Activity className="size-4" />
          </div>
          <div className="min-w-0">
            <h2 className="text-xs font-bold tracking-tight text-foreground">控制台轨迹与检查器</h2>
            <p className="truncate font-mono text-[10px] text-muted-foreground">{sessionId.slice(0, 14)}…</p>
          </div>
        </div>
        <Button variant="ghost" size="icon-sm" onClick={onClose} aria-label="关闭检查器">
          <X className="size-4" />
        </Button>
      </header>

      {/* Tabs */}
      <div className="flex items-center gap-1 border-b border-border/80 bg-muted/30 px-3 py-1.5">
        <button
          type="button"
          onClick={() => setActiveTab("timeline")}
          className={cn(
            "flex-1 rounded-lg py-1 text-xs font-medium transition-all text-center",
            activeTab === "timeline" ? "bg-background font-bold text-foreground shadow-2xs border border-border/70" : "text-muted-foreground hover:text-foreground",
          )}
        >
          01 轨迹 (Timeline)
        </button>
        <button
          type="button"
          onClick={() => setActiveTab("context")}
          className={cn(
            "flex-1 rounded-lg py-1 text-xs font-medium transition-all text-center",
            activeTab === "context" ? "bg-background font-bold text-foreground shadow-2xs border border-border/70" : "text-muted-foreground hover:text-foreground",
          )}
        >
          02 上下文监视
        </button>
      </div>

      {/* Sub Toolbar for Timeline */}
      {activeTab === "timeline" && (
        <div className="flex shrink-0 items-center justify-between border-b border-border/70 bg-muted/20 px-4 py-2 text-xs font-mono text-muted-foreground">
          <button
            type="button"
            onClick={() => setHideHeartbeat((value) => !value)}
            className="inline-flex items-center gap-1.5 hover:text-foreground transition-colors"
          >
            <Radio className="size-3.5 text-blue-500" />
            {hideHeartbeat ? "只看关键轨迹" : "显示全部心跳"}
          </button>
          <div className="flex items-center gap-2">
            <span className="inline-flex items-center gap-1 text-[11px] text-emerald-600 dark:text-emerald-400 font-bold">
              <span className="size-1.5 rounded-full bg-emerald-500" />
              {shown.length} 节点
            </span>
            <Button
              variant="ghost"
              size="icon-sm"
              className="h-6 w-6"
              onClick={() => setFrames([])}
              title="清空记录"
            >
              <Trash2 className="size-3.5" />
            </Button>
          </div>
        </div>
      )}

      {/* Main Content Area */}
      <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto p-4" aria-live="polite">
        {activeTab === "timeline" && (
          shown.length > 0 ? (
            <div className="pt-2">
              {shown.map((frame, index) => (
                <TimelineEventNode
                  key={`${frame.ts}-${index}`}
                  frame={frame}
                  isLast={index === shown.length - 1}
                />
              ))}
            </div>
          ) : (
            <div className="flex h-full flex-col items-center justify-center gap-3 py-16 text-center">
              <div className="flex size-12 items-center justify-center rounded-2xl bg-blue-500/10 text-blue-500">
                <Activity className="size-6" />
              </div>
              <div>
                <p className="text-xs font-bold text-foreground">等待智能体运行轨迹...</p>
                <p className="mt-1 text-[11px] leading-relaxed text-muted-foreground max-w-xs">
                  模型思考、工具调起、会话状态切换事件将实时在此展示。
                </p>
              </div>
            </div>
          )
        )}

        {activeTab === "context" && (
          <div className="space-y-4">
            <div className="rounded-2xl border border-border/70 bg-card p-4 space-y-3 shadow-2xs">
              <div className="flex items-center gap-2 text-xs font-bold text-foreground">
                <Sparkles className="size-4 text-blue-500" />
                <span>当前会话算力与上下文监视</span>
              </div>
              <div className="grid grid-cols-2 gap-2 text-xs font-mono pt-1">
                <div className="rounded-xl border border-border/60 bg-muted/30 p-2.5">
                  <span className="text-[10px] text-muted-foreground block">会话 ID</span>
                  <span className="font-bold text-foreground truncate block mt-0.5">{sessionId}</span>
                </div>
                <div className="rounded-xl border border-border/60 bg-muted/30 p-2.5">
                  <span className="text-[10px] text-muted-foreground block">事件推流</span>
                  <span className="font-bold text-emerald-600 dark:text-emerald-400 flex items-center gap-1 mt-0.5">
                    <CheckCircle2 className="size-3" /> SSE 联通
                  </span>
                </div>
              </div>
            </div>
          </div>
        )}
      </div>

      <footer className="flex shrink-0 items-center gap-2 border-t border-border/80 px-4 py-2.5 text-[11px] font-mono text-muted-foreground bg-muted/20">
        <span className="size-2 rounded-full bg-emerald-500 animate-pulse" />
        智能体控制平面 · 实时事件链路监视中
      </footer>
    </aside>
  );
}
