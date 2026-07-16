"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AlertTriangle, ArrowUp, Check, ChevronDown, FileText, Hand, ShieldCheck, Square } from "lucide-react";
import { sendMessage, type ApprovalMode } from "@/lib/api";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useAutosizeTextarea } from "@/lib/hooks/use-autosize-textarea";

const APPROVAL_MODES: { value: ApprovalMode; label: string; description: string; icon: typeof Hand }[] = [
  {
    value: "ask",
    label: "请求批准",
    description: "编辑外部文件和使用互联网时始终询问",
    icon: Hand,
  },
  {
    value: "auto",
    label: "替我审批",
    description: "仅对检测到的风险操作请求批准",
    icon: ShieldCheck,
  },
  {
    value: "full",
    label: "完全访问权限",
    description: "可不受限地访问互联网和您电脑上的任何文件",
    icon: AlertTriangle,
  },
];

function ApprovalModeSelect({
  mode,
  onChange,
}: {
  mode: ApprovalMode;
  onChange: (mode: ApprovalMode) => void;
}) {
  const current = APPROVAL_MODES.find((item) => item.value === mode) ?? APPROVAL_MODES[0];
  const CurrentIcon = current.icon;
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        className={`inline-flex shrink-0 items-center gap-1 rounded-md px-1.5 py-0.5 text-xs transition-colors hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50 ${
          mode === "full"
            ? "font-medium text-orange-600 dark:text-orange-400"
            : "text-muted-foreground"
        }`}
        aria-label="审批模式"
      >
        <CurrentIcon className="size-3.5" />
        {mode === "full" ? "完全访问" : current.label}
        <ChevronDown className="size-3" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-80 p-1.5">
        <div className="px-2 pb-1.5 pt-1 text-xs text-muted-foreground">应如何批准智能体操作？</div>
        {APPROVAL_MODES.map((item) => {
          const Icon = item.icon;
          const selected = item.value === mode;
          return (
            <DropdownMenuItem
              key={item.value}
              onClick={() => onChange(item.value)}
              className="items-start gap-3 px-2.5 py-2"
            >
              <Icon className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
              <div className="min-w-0 flex-1">
                <div className="text-sm font-medium">{item.label}</div>
                <div className="text-xs text-muted-foreground">{item.description}</div>
              </div>
              {selected && <Check className="mt-1 size-4 shrink-0" />}
            </DropdownMenuItem>
          );
        })}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

// Extracts an "@token" being typed at the caret, e.g. "看下 @src/ma".
function mentionQueryAt(text: string, caret: number): { query: string; start: number } | null {
  const upToCaret = text.slice(0, caret);
  const match = /(^|\s)@([^\s@]*)$/.exec(upToCaret);
  if (!match) return null;
  return { query: match[2], start: caret - match[2].length - 1 };
}

export function Composer({
  sessionId,
  model,
  onSent,
  onSend,
  onSendStart,
  onAbort,
  busy = false,
  disabled = false,
  disabledHint,
  draftValue,
  onDraftChange,
  focusVersion,
  mentionFiles,
  approvalMode,
  onApprovalModeChange,
}: {
  sessionId: string;
  model: string;
  onSent?: () => void;
  onSend?: (text: string) => Promise<void>;
  onSendStart?: (text: string) => void;
  onAbort?: () => void;
  busy?: boolean;
  disabled?: boolean;
  disabledHint?: string;
  draftValue?: string;
  onDraftChange?: React.Dispatch<React.SetStateAction<string>>;
  focusVersion?: number;
  /** Workspace file paths offered when the user types "@". */
  mentionFiles?: string[];
  approvalMode?: ApprovalMode;
  onApprovalModeChange?: (mode: ApprovalMode) => void;
}) {
  const [localDraft, setLocalDraft] = useState("");
  const draft = draftValue ?? localDraft;
  const setDraft = onDraftChange ?? setLocalDraft;
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const textareaRef = useAutosizeTextarea(draft);

  // ↑/↓ recall of previously sent prompts (only when the draft is empty or
  // already navigating history, so normal multi-line editing is unaffected).
  const historyRef = useRef<string[]>([]);
  const [historyIndex, setHistoryIndex] = useState<number | null>(null);

  // @-mention state.
  const [mention, setMention] = useState<{ query: string; start: number } | null>(null);
  const [mentionIndex, setMentionIndex] = useState(0);
  const mentionMatches = useMemo(() => {
    if (!mention || !mentionFiles?.length) return [];
    const q = mention.query.toLowerCase();
    return mentionFiles
      .filter((path) => path.toLowerCase().includes(q))
      .slice(0, 8);
  }, [mention, mentionFiles]);

  useEffect(() => {
    if (!focusVersion) return;
    textareaRef.current?.focus();
  }, [focusVersion, textareaRef]);

  const refreshMention = useCallback((value: string) => {
    const caret = textareaRef.current?.selectionStart ?? value.length;
    const next = mentionFiles?.length ? mentionQueryAt(value, caret) : null;
    setMention(next);
    setMentionIndex(0);
  }, [mentionFiles, textareaRef]);

  const insertMention = useCallback((path: string) => {
    if (!mention) return;
    const caret = textareaRef.current?.selectionStart ?? draft.length;
    const next = `${draft.slice(0, mention.start)}@${path} ${draft.slice(caret)}`;
    setDraft(next);
    setMention(null);
    requestAnimationFrame(() => textareaRef.current?.focus());
  }, [draft, mention, setDraft, textareaRef]);

  const handleSend = useCallback(async () => {
    const t = draft.trim();
    if (!t || sending || disabled) return;
    setSending(true);
    setError(null);
    onSendStart?.(t);
    try {
      await (onSend ? onSend(t) : sendMessage({ sessionId, text: t, model }));
      historyRef.current = [...historyRef.current.filter((item) => item !== t), t].slice(-50);
      setHistoryIndex(null);
      setDraft((current) => (current.trim() === t ? "" : current));
      onSent?.();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSending(false);
    }
  }, [draft, sending, disabled, sessionId, model, onSent, onSend, onSendStart, setDraft]);

  // Plain Enter sends, Shift+Enter inserts a newline. Matches LAP.
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (mention && mentionMatches.length > 0) {
        if (e.key === "ArrowDown") {
          e.preventDefault();
          setMentionIndex((i) => (i + 1) % mentionMatches.length);
          return;
        }
        if (e.key === "ArrowUp") {
          e.preventDefault();
          setMentionIndex((i) => (i - 1 + mentionMatches.length) % mentionMatches.length);
          return;
        }
        if (e.key === "Enter" || e.key === "Tab") {
          e.preventDefault();
          insertMention(mentionMatches[mentionIndex]);
          return;
        }
        if (e.key === "Escape") {
          e.preventDefault();
          setMention(null);
          return;
        }
      }
      if (e.key === "ArrowUp" && (draft === "" || historyIndex !== null)) {
        const history = historyRef.current;
        if (history.length > 0) {
          e.preventDefault();
          const next = historyIndex === null ? history.length - 1 : Math.max(0, historyIndex - 1);
          setHistoryIndex(next);
          setDraft(history[next]);
          return;
        }
      }
      if (e.key === "ArrowDown" && historyIndex !== null) {
        e.preventDefault();
        const history = historyRef.current;
        if (historyIndex >= history.length - 1) {
          setHistoryIndex(null);
          setDraft("");
        } else {
          setHistoryIndex(historyIndex + 1);
          setDraft(history[historyIndex + 1]);
        }
        return;
      }
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        void handleSend();
      }
    },
    [handleSend, mention, mentionMatches, mentionIndex, insertMention, draft, historyIndex, setDraft],
  );

  const canSend = draft.trim().length > 0 && !sending && !disabled;
  const placeholder = sending
    ? "发送中..."
    : disabled
      ? (disabledHint ?? "等待运行时就绪...")
      : busy
        ? "发送将打断当前运行并转向新指令"
    : "输入消息...";

  return (
    <div className="border-t border-border bg-background/95 backdrop-blur">
      <div className="mx-auto max-w-5xl px-6 py-4">
        <div className="relative">
          {mention && mentionMatches.length > 0 && (
            <div className="absolute bottom-full left-0 z-20 mb-1 w-full max-w-md overflow-hidden rounded-lg border border-border bg-popover shadow-md">
              {mentionMatches.map((path, index) => (
                <button
                  key={path}
                  type="button"
                  onMouseDown={(e) => {
                    e.preventDefault();
                    insertMention(path);
                  }}
                  className={`flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs ${
                    index === mentionIndex ? "bg-muted text-foreground" : "text-muted-foreground"
                  }`}
                >
                  <FileText className="size-3 shrink-0" />
                  <span className="mono truncate">{path}</span>
                </button>
              ))}
            </div>
          )}
          <div className="overflow-hidden rounded-2xl border border-border bg-card shadow-sm transition-all focus-within:border-ring focus-within:ring-1 focus-within:ring-ring">
            <textarea
              id="chat-composer"
              ref={textareaRef}
              value={draft}
              onChange={(e) => {
                setDraft(e.target.value);
                if (historyIndex !== null) setHistoryIndex(null);
                refreshMention(e.target.value);
              }}
              onKeyDown={handleKeyDown}
              onBlur={() => setMention(null)}
              placeholder={placeholder}
              disabled={disabled}
              rows={1}
              className="min-h-14 w-full resize-none bg-transparent px-4 pt-4 text-[15px] text-foreground outline-none focus-visible:outline-none placeholder:text-muted-foreground"
            />
            <div className="flex items-center justify-between px-4 pb-3 text-xs text-muted-foreground">
              <span className="flex min-w-0 items-center gap-2 truncate">
                {approvalMode && onApprovalModeChange && (
                  <ApprovalModeSelect mode={approvalMode} onChange={onApprovalModeChange} />
                )}
                <span className="mono min-w-0 truncate">
                  {error ? (
                    <span className="text-red-600 dark:text-red-400">{error}</span>
                  ) : (
                    model || "Enter to send · Shift+Enter for newline"
                  )}
                </span>
              </span>
              <div className="flex items-center gap-2">
                {busy && onAbort && !draft.trim() ? (
                  <button
                    type="button"
                    onClick={onAbort}
                    className="rounded-full bg-red-600 p-1.5 text-white transition-colors hover:bg-red-700"
                    aria-label="停止智能体"
                    title="停止（中断智能体）"
                  >
                    <Square className="w-3.5 h-3.5 fill-current" />
                  </button>
                ) : (
                  <button
                    type="button"
                    onClick={() => void handleSend()}
                    disabled={!canSend}
                    className="rounded-full bg-foreground p-1.5 text-background transition-colors hover:bg-foreground/90 disabled:opacity-30 disabled:hover:bg-foreground"
                    aria-label="发送"
                    title="发送（Enter）"
                  >
                    <ArrowUp className="w-3.5 h-3.5" />
                  </button>
                )}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
