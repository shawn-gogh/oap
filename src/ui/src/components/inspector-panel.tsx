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
import { Activity, ChevronRight, Radio, Trash2, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { runtimeEventSourceUrl } from "@/lib/api";

const DEFAULT_PANEL_WIDTH = 420;
const MIN_PANEL_WIDTH = 320;
const MAX_PANEL_WIDTH = 720;

interface OcEvent {
  type: string;
  [key: string]: unknown;
}

export interface Frame {
  ts: number;
  ev: OcEvent;
}

function summarize(ev: OcEvent): string {
  const text =
    typeof ev.text === "string"
      ? ev.text
      : typeof ev.delta === "string"
        ? ev.delta
        : "";
  switch (ev.type) {
    case "session.status_idle":
      return "智能体已返回控制权";
    case "session.error":
      return String(
        typeof ev.error === "object" && ev.error
          ? ((ev.error as { message?: unknown }).message ?? "运行错误")
          : (ev.error ?? "运行错误"),
      );
    case "assistant_response":
    case "thinking_back":
    case "agent.message":
    case "agent.thinking":
    case "agent.reasoning":
      return text ? text.replace(/\s+/g, " ").slice(0, 96) : "无文本负载";
    case "session.status": {
      const status = ev.status as { type?: string } | string | undefined;
      return typeof status === "string" ? status : (status?.type ?? "状态已更新");
    }
    default:
      return JSON.stringify(ev).slice(0, 96);
  }
}

function eventTone(type: string): string {
  if (type === "session.error") return "bg-red-500";
  if (type === "session.status_idle") return "bg-emerald-500";
  if (type === "session.status") return "bg-amber-500";
  return "bg-muted-foreground";
}

function fmtTime(ts: number): string {
  return new Date(ts).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

function EventRow({ frame }: { frame: Frame }) {
  const [open, setOpen] = useState(false);

  return (
    <div className="border-b border-border/70 last:border-b-0">
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        aria-expanded={open}
        className="flex w-full items-start gap-2 px-3 py-2 text-left transition-colors hover:bg-muted/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/50"
      >
        <span className={`mt-1.5 size-1.5 shrink-0 rounded-full ${eventTone(frame.ev.type)}`} />
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-center gap-2">
            <span className="truncate font-mono text-[11px] font-medium text-foreground">{frame.ev.type}</span>
            <span className="ml-auto shrink-0 font-mono text-[10px] text-muted-foreground">{fmtTime(frame.ts)}</span>
          </div>
          <p className="mt-0.5 truncate text-[11px] leading-4 text-muted-foreground">{summarize(frame.ev)}</p>
        </div>
        <ChevronRight className={`mt-1 size-3.5 shrink-0 text-muted-foreground transition-transform ${open ? "rotate-90" : ""}`} />
      </button>
      {open && (
        <pre className="mx-3 mb-3 max-h-56 overflow-auto rounded-md border border-border bg-muted/30 p-2.5 font-mono text-[11px] leading-5 text-muted-foreground whitespace-pre-wrap break-words">
          {JSON.stringify(frame.ev, null, 2)}
        </pre>
      )}
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

  const handlePanelResizeKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "ArrowLeft") {
      event.preventDefault();
      resizePanel(panelWidth + 24);
    } else if (event.key === "ArrowRight") {
      event.preventDefault();
      resizePanel(panelWidth - 24);
    } else if (event.key === "Home") {
      event.preventDefault();
      resizePanel(MIN_PANEL_WIDTH);
    } else if (event.key === "End") {
      event.preventDefault();
      resizePanel(MAX_PANEL_WIDTH);
    }
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
  // eslint-disable-next-line react-hooks/exhaustive-deps
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
      className="fixed inset-y-0 right-0 z-40 flex w-[min(420px,calc(100vw-1rem))] min-w-0 flex-col border-l border-border bg-background shadow-xl xl:relative xl:inset-auto xl:z-auto xl:h-screen xl:w-[var(--inspector-panel-width)] xl:shrink-0 xl:shadow-none"
      style={{ "--inspector-panel-width": `${panelWidth}px` } as CSSProperties}
    >
      <div
        role="separator"
        aria-label="调整检查器宽度"
        aria-orientation="vertical"
        aria-valuemin={MIN_PANEL_WIDTH}
        aria-valuemax={MAX_PANEL_WIDTH}
        aria-valuenow={panelWidth}
        tabIndex={0}
        onPointerDown={startPanelResize}
        onKeyDown={handlePanelResizeKeyDown}
        onDoubleClick={() => setPanelWidth(DEFAULT_PANEL_WIDTH)}
        className="group absolute inset-y-0 -left-1 z-50 hidden w-2 cursor-col-resize touch-none xl:block focus-visible:outline-none"
        title="拖动调整宽度，双击复位"
      >
        <span className="mx-auto block h-full w-px bg-transparent transition-colors group-hover:bg-primary group-focus-visible:bg-primary" />
      </div>
      <header className="flex h-12 shrink-0 items-center gap-2 border-b border-border px-3">
        <span className="flex size-7 items-center justify-center rounded-md bg-muted text-muted-foreground">
          <Activity className="size-4" />
        </span>
        <div className="min-w-0">
          <h2 className="text-[13.5px] font-semibold tracking-tight">检查器</h2>
          <p className="truncate font-mono text-[10px] text-muted-foreground">{sessionId.slice(0, 12)}…</p>
        </div>
        <Button variant="ghost" size="icon-sm" className="ml-auto" onClick={onClose} aria-label="关闭检查器">
          <X className="size-4" />
        </Button>
      </header>

      <div className="flex shrink-0 items-center gap-2 border-b border-border bg-muted/30 px-3 py-2">
        <button
          type="button"
          onClick={() => setHideHeartbeat((value) => !value)}
          aria-pressed={hideHeartbeat}
          className="inline-flex items-center gap-1.5 rounded-md px-1.5 py-1 text-[11px] text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
        >
          <Radio className="size-3.5" />
          {hideHeartbeat ? "仅显示重要事件" : "显示全部事件"}
        </button>
        <span className="ml-auto inline-flex items-center gap-1.5 font-mono text-[11px] text-muted-foreground">
          <span className="size-1.5 rounded-full bg-emerald-500" />
          {shown.length}
        </span>
        <Button
          variant="ghost"
          size="icon-sm"
          onClick={() => setFrames([])}
          aria-label="清空检查器事件"
          title="清空事件"
        >
          <Trash2 className="size-3.5" />
        </Button>
      </div>

      <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto" aria-live="polite">
        {shown.length > 0 ? (
          shown.map((frame, index) => <EventRow key={`${frame.ts}-${index}`} frame={frame} />)
        ) : (
          <div className="flex h-full flex-col items-center justify-center gap-3 px-8 text-center">
            <span className="flex size-10 items-center justify-center rounded-xl bg-muted text-muted-foreground">
              <Activity className="size-5" />
            </span>
            <div>
              <p className="text-sm font-medium text-foreground">等待运行事件</p>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">新的模型、工具和会话事件会实时显示在这里。</p>
            </div>
          </div>
        )}
      </div>

      <footer className="flex shrink-0 items-center gap-1.5 border-t border-border px-3 py-2 text-[10px] text-muted-foreground">
        <span className="size-1.5 rounded-full bg-emerald-500" />
        已连接到运行时事件流
      </footer>
    </aside>
  );
}
