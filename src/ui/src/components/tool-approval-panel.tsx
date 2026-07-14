"use client";

import { useMemo, useState } from "react";
import { Check, Copy, RotateCcw, Send, XCircle } from "lucide-react";
import { Button } from "@/components/ui/button";
import type { PendingApproval } from "@/lib/api";

function toFieldLabel(key: string): string {
  return key.replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
}

function toStringValue(v: unknown): string {
  if (v === null || v === undefined) return "";
  if (typeof v === "string") return v;
  return JSON.stringify(v, null, 2);
}

// Keep edited JSON-like values typed when the original argument was typed.
function fromStringValue(original: unknown, text: string): unknown {
  if (typeof original === "string" || original === null || original === undefined) return text;
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

export interface ToolApprovalPanelProps {
  approval: PendingApproval;
  onAccept: (id: string, args: Record<string, unknown>) => void;
  onReject: (id: string, feedback: string) => void;
  onAcceptAlways?: (id: string, args: Record<string, unknown>) => void;
  busy?: boolean;
}

export function ToolApprovalPanel({ approval, onAccept, onReject, onAcceptAlways, busy }: ToolApprovalPanelProps) {
  const initial = useMemo<Record<string, string>>(() => {
    const out: Record<string, string> = {};
    for (const [k, v] of Object.entries(approval.arguments ?? {})) out[k] = toStringValue(v);
    return out;
  }, [approval]);

  const [fields, setFields] = useState<Record<string, string>>(initial);
  const [feedback, setFeedback] = useState("");
  const [copied, setCopied] = useState(false);

  const keys = Object.keys(approval.arguments ?? {}).filter(
    (k) => k !== "options" && k !== "choices"
  );

  const options = useMemo<string[]>(() => {
    const opts = approval.arguments?.options ?? approval.arguments?.choices;
    if (Array.isArray(opts)) {
      return opts.map((o) => (typeof o === "string" ? o : JSON.stringify(o)));
    }
    return [];
  }, [approval]);

  const [selectedOption, setSelectedOption] = useState<string | null>(null);

  const dirty = keys.some((k) => fields[k] !== initial[k]) || selectedOption !== null;

  const buildArgs = (): Record<string, unknown> => {
    const out: Record<string, unknown> = {};
    for (const k of keys) out[k] = fromStringValue(approval.arguments[k], fields[k] ?? "");
    if (selectedOption) {
      out["choice"] = selectedOption;
      out["selected_option"] = selectedOption;
    }
    return out;
  };

  const handleSelectOption = (opt: string) => {
    setSelectedOption(opt);
    setFeedback(opt);
    setFields((f) => ({
      ...f,
      choice: opt,
      selected_option: opt,
    }));
  };

  const handleReset = () => {
    setFields(initial);
    setSelectedOption(null);
    setFeedback("");
  };

  const copyName = async () => {
    try {
      await navigator.clipboard.writeText(approval.tool);
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    } catch {
      /* noop */
    }
  };

  return (
    <div className="rounded-lg border border-border bg-card">
      <div className="flex flex-col gap-3 border-b border-border px-4 py-4 sm:flex-row sm:items-center">
        <div className="flex size-9 shrink-0 items-center justify-center rounded-md border border-border bg-muted/40">
          <span className="size-2 rounded-full bg-amber-400" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="text-[11px] font-medium uppercase text-muted-foreground">需要人工审批</div>
          <div className="mt-1 truncate text-base font-semibold">{approval.tool}</div>
        </div>
        <Button variant="outline" size="sm" onClick={copyName}>
          {copied ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
          {copied ? "已复制" : "复制工具名"}
        </Button>
      </div>

      {options.length > 0 && (
        <div className="border-b border-border px-4 py-4 bg-muted/10">
          <div className="text-xs font-semibold uppercase tracking-wider text-muted-foreground mb-3 flex items-center gap-1.5">
            <span className="size-1.5 rounded-full bg-amber-500 animate-pulse" />
            建议选项
          </div>
          <div className="flex flex-wrap gap-2">
            {options.map((opt) => {
              const isSelected = selectedOption === opt;
              return (
                <button
                  key={opt}
                  type="button"
                  onClick={() => handleSelectOption(opt)}
                  disabled={busy}
                  className={`px-4 py-2 rounded-lg text-sm font-medium border transition-all duration-200 shadow-sm flex items-center gap-2 ${
                    isSelected
                      ? "bg-amber-500/10 border-amber-500 text-amber-900 dark:text-amber-100 ring-2 ring-amber-500/30 scale-[1.02]"
                      : "bg-background border-border text-muted-foreground hover:bg-muted/50 hover:text-foreground hover:border-muted-foreground/30 active:scale-95"
                  }`}
                >
                  {isSelected && <span className="size-1.5 rounded-full bg-amber-500" />}
                  {opt}
                </button>
              );
            })}
          </div>
          <div className="mt-2 text-xs text-muted-foreground/80">
            选择一个选项后：拒绝时它将作为反馈，批准时它将作为通过的参数。
          </div>
        </div>
      )}

      <div className="grid gap-4 p-4 2xl:grid-cols-[minmax(0,1fr)_320px]">
        <div className="rounded-md border border-border bg-background">
          <div className="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
            <div>
              <div className="text-sm font-medium">参数</div>
              <div className="text-xs text-muted-foreground">可在允许智能体继续前修改参数值。</div>
            </div>
            <Button
              variant="ghost"
              size="sm"
              onClick={handleReset}
              disabled={busy || !dirty}
            >
              <RotateCcw className="size-3.5" />
              重置
            </Button>
          </div>

          <div className="space-y-3 p-4">
            {keys.length === 0 ? (
              <div className="rounded-md border border-border bg-muted/20 px-3 py-8 text-center text-sm text-muted-foreground">
                此操作不需要参数。
              </div>
            ) : (
              keys.map((k) => (
                <ArgumentField
                  key={k}
                  name={k}
                  value={fields[k] ?? ""}
                  onChange={(value) => setFields((f) => ({ ...f, [k]: value }))}
                  disabled={busy}
                />
              ))
            )}
          </div>
        </div>

        <div className="flex flex-col rounded-md border border-border bg-background">
          <div className="border-b border-border px-4 py-3">
            <div className="text-sm font-medium">决定</div>
            <div className="text-xs text-muted-foreground">批准将按编辑后的参数执行；拒绝会把反馈返回给智能体。</div>
          </div>

          <div className="flex flex-1 flex-col gap-3 p-4">
            <Button onClick={() => onAccept(approval.id, buildArgs())} disabled={busy}>
              <Send className="size-3.5" />
              仅本次通过
              <span className="mono ml-1 rounded border border-current/25 px-1 text-[10px] opacity-70">Y</span>
            </Button>
            {approval.kind === "tool_permission" && onAcceptAlways && (
              <Button
                variant="outline"
                onClick={() => onAcceptAlways(approval.id, buildArgs())}
                disabled={busy}
                title="本会话内匹配当前权限规则的后续操作将自动通过"
              >
                <Check className="size-3.5" />
                本会话允许同类操作
              </Button>
            )}

            <div className="h-px bg-border" />

            <label className="text-xs font-medium text-muted-foreground" htmlFor={`reject-${approval.id}`}>
              拒绝原因
            </label>
            <textarea
              id={`reject-${approval.id}`}
              value={feedback}
              onChange={(e) => setFeedback(e.target.value)}
              rows={5}
              placeholder="告诉智能体重试前需要调整什么..."
              className="min-h-28 w-full flex-1 resize-y rounded-md border border-input bg-background px-3 py-2 text-sm leading-5 outline-none transition-colors focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
              disabled={busy}
            />

            <Button
              variant="destructive"
              onClick={() => onReject(approval.id, feedback.trim())}
              disabled={busy}
            >
              <XCircle className="size-3.5" />
              拒绝
              <span className="mono ml-1 rounded border border-current/25 px-1 text-[10px] opacity-70">N</span>
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}

function ArgumentField({
  name,
  value,
  onChange,
  disabled,
}: {
  name: string;
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
}) {
  return (
    <div className="space-y-1.5">
      <label className="flex items-center justify-between gap-2 text-xs">
        <span className="font-medium text-muted-foreground">{toFieldLabel(name)}</span>
        <span className="truncate font-mono text-[11px] text-muted-foreground/70">{name}</span>
      </label>
      <textarea
        value={value}
        onChange={(e) => onChange(e.target.value)}
        rows={value.includes("\n") ? Math.min(value.split("\n").length, 10) : 2}
        className="min-h-11 w-full resize-y rounded-md border border-input bg-background px-3 py-2 font-mono text-xs leading-5 outline-none transition-colors focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
        disabled={disabled}
      />
    </div>
  );
}
