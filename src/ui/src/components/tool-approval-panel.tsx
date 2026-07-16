"use client";

import { useMemo, useState } from "react";
import {
  Ban,
  Check,
  CheckCheck,
  Clock3,
  Copy,
  Loader2,
  RotateCcw,
  ShieldAlert,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import type { PendingApproval } from "@/lib/api";

function toFieldLabel(key: string): string {
  return key.replace(/_/g, " ").replace(/\b\w/g, (character) => character.toUpperCase());
}

function toStringValue(value: unknown): string {
  if (value === null || value === undefined) return "";
  if (typeof value === "string") return value;
  return JSON.stringify(value, null, 2);
}

function fromStringValue(original: unknown, text: string): unknown {
  if (typeof original === "string" || original === null || original === undefined) return text;
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

function approvalKindLabel(kind: PendingApproval["kind"]): string {
  if (kind === "business_decision") return "业务决策";
  if (kind === "data_egress" || kind === "unlisted_data_egress") return "数据外发";
  if (kind === "runtime_permission") return "运行时权限";
  if (kind === "tool_permission") return "工具权限";
  if (kind === "agent_publish") return "发布审批";
  if (kind === "agent_change") return "配置变更";
  if (kind === "platform_action") return "平台操作";
  return "操作确认";
}

function approvalTime(createdAt: number): string {
  if (!createdAt) return "刚刚";
  return new Date(createdAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

export interface ToolApprovalPanelProps {
  approval: PendingApproval;
  onAccept: (id: string, args: Record<string, unknown>) => void;
  onReject: (id: string, feedback: string) => void;
  onAcceptAlways?: (id: string, args: Record<string, unknown>) => void;
  busy?: boolean;
  canDecide?: boolean;
}

export function ToolApprovalPanel({
  approval,
  onAccept,
  onReject,
  onAcceptAlways,
  busy,
  canDecide = true,
}: ToolApprovalPanelProps) {
  const argumentsEditable = approval.kind === "approval" || approval.kind === "business_decision";
  const initial = useMemo<Record<string, string>>(() => {
    const values: Record<string, string> = {};
    for (const [key, value] of Object.entries(approval.arguments ?? {})) {
      values[key] = toStringValue(value);
    }
    return values;
  }, [approval]);

  const [fields, setFields] = useState<Record<string, string>>(initial);
  const [feedback, setFeedback] = useState("");
  const [copied, setCopied] = useState(false);
  const [rejectOpen, setRejectOpen] = useState(false);
  const [selectedOption, setSelectedOption] = useState<string | null>(null);

  const keys = Object.keys(approval.arguments ?? {}).filter(
    (key) => key !== "options" && key !== "choices",
  );
  const options = useMemo<string[]>(() => {
    const values = approval.arguments?.options ?? approval.arguments?.choices;
    if (!Array.isArray(values)) return [];
    return values.map((value) => (typeof value === "string" ? value : JSON.stringify(value)));
  }, [approval]);
  const dirty = keys.some((key) => fields[key] !== initial[key]) || selectedOption !== null;

  const buildArgs = (): Record<string, unknown> => {
    const values: Record<string, unknown> = {};
    for (const key of keys) {
      values[key] = fromStringValue(approval.arguments[key], fields[key] ?? "");
    }
    if (selectedOption) {
      values.choice = selectedOption;
      values.selected_option = selectedOption;
    }
    return values;
  };

  const handleSelectOption = (option: string) => {
    setSelectedOption(option);
    setFeedback(option);
    setFields((current) => ({ ...current, choice: option, selected_option: option }));
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
      window.setTimeout(() => setCopied(false), 1200);
    } catch {
      setCopied(false);
    }
  };

  return (
    <section className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-xl border border-amber-500/25 bg-card shadow-lg shadow-amber-950/5">
      <div className="shrink-0 flex items-start gap-3 border-b border-amber-500/15 bg-amber-500/5 px-4 py-3.5 sm:px-5">
        <div className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-amber-500/15 text-amber-600 dark:text-amber-400">
          <ShieldAlert className="size-4" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-[11px] text-muted-foreground">
            <span className="font-semibold uppercase tracking-wide text-amber-700 dark:text-amber-300">
              {approvalKindLabel(approval.kind)}
            </span>
            <span aria-hidden>·</span>
            <span className="inline-flex items-center gap-1">
              <Clock3 className="size-3" />
              {approvalTime(approval.createdAt)}
            </span>
          </div>
          <div className="mt-0.5 truncate text-sm font-semibold text-foreground">{approval.tool}</div>
          <p className="mt-0.5 text-xs text-muted-foreground">
            {canDecide ? "需要你确认后才能继续" : "等待有权限的审批人处理"}
          </p>
        </div>
        <Button variant="ghost" size="icon-sm" onClick={copyName} aria-label="复制操作名称">
          {copied ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
        </Button>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain [scrollbar-gutter:stable]">
        <div className="mx-auto w-full max-w-4xl space-y-4 px-4 py-4 sm:px-5">
          {options.length > 0 && (
            <div>
              <div className="mb-2 text-xs font-medium text-foreground">请选择一个处理方案</div>
              <div className="grid gap-2 sm:grid-cols-2">
                {options.map((option) => {
                  const selected = selectedOption === option;
                  return (
                    <button
                      key={option}
                      type="button"
                      onClick={() => handleSelectOption(option)}
                      disabled={busy || !canDecide}
                      className={`flex min-h-10 items-center gap-2 rounded-lg border px-3 py-2 text-left text-sm transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50 disabled:cursor-not-allowed disabled:opacity-50 ${
                        selected
                          ? "border-amber-500/60 bg-amber-500/10 text-foreground"
                          : "border-border bg-background text-muted-foreground hover:bg-muted/50 hover:text-foreground"
                      }`}
                    >
                      <span
                        className={`flex size-4 shrink-0 items-center justify-center rounded-full border ${
                          selected ? "border-amber-500 bg-amber-500 text-white" : "border-border"
                        }`}
                      >
                        {selected && <Check className="size-3" />}
                      </span>
                      <span className="break-words">{option}</span>
                    </button>
                  );
                })}
              </div>
            </div>
          )}

          <div className="overflow-hidden rounded-lg border border-border bg-background">
            <div className="flex items-center justify-between gap-3 border-b border-border bg-muted/30 px-3 py-2.5">
              <div>
                <div className="text-xs font-medium text-foreground">请求详情</div>
                <div className="text-[11px] text-muted-foreground">
                  {argumentsEditable ? "批准前可以调整参数" : "参数只读，保留为审计证据"}
                </div>
              </div>
              {argumentsEditable && keys.length > 0 && (
                <Button variant="ghost" size="sm" onClick={handleReset} disabled={busy || !dirty}>
                  <RotateCcw className="size-3.5" />
                  重置
                </Button>
              )}
            </div>
            <div className="space-y-3 p-3">
              {keys.length === 0 ? (
                <p className="py-3 text-center text-xs text-muted-foreground">此操作不包含额外参数。</p>
              ) : (
                keys.map((key) => (
                  <ArgumentField
                    key={key}
                    name={key}
                    value={fields[key] ?? ""}
                    onChange={(value) => setFields((current) => ({ ...current, [key]: value }))}
                    disabled={busy || !argumentsEditable || !canDecide}
                  />
                ))
              )}
            </div>
          </div>
        </div>
      </div>

      <div className="shrink-0 border-t border-border bg-muted/20 px-4 py-3 sm:px-5">
        {!canDecide ? (
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <ShieldAlert className="size-4 shrink-0 text-amber-600 dark:text-amber-400" />
            <span>当前账号没有处理这条审批的权限。</span>
          </div>
        ) : (
          <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-end">
            <Button
              variant="ghost"
              className="text-destructive hover:bg-destructive/10 hover:text-destructive sm:mr-auto"
              onClick={() => setRejectOpen(true)}
              disabled={busy}
            >
              <Ban className="size-3.5" />
              拒绝或要求修改
            </Button>
            {(approval.kind === "tool_permission" || approval.kind === "runtime_permission") && onAcceptAlways && (
              <Button variant="outline" onClick={() => onAcceptAlways(approval.id, buildArgs())} disabled={busy}>
                <CheckCheck className="size-3.5" />
                本会话允许同类操作
              </Button>
            )}
            <Button onClick={() => onAccept(approval.id, buildArgs())} disabled={busy}>
              {busy ? (
                <Loader2 className="size-3.5 animate-spin motion-reduce:animate-none" />
              ) : (
                <Check className="size-3.5" />
              )}
              允许本次操作
              <span className="ml-1 rounded border border-current/20 px-1 font-mono text-[10px] opacity-60">Y</span>
            </Button>
          </div>
        )}
      </div>

      <Dialog open={rejectOpen} onOpenChange={setRejectOpen}>
        <DialogContent showCloseButton={!busy} className="max-w-md gap-5">
          <DialogHeader>
            <DialogTitle>拒绝本次操作</DialogTitle>
            <DialogDescription>
              这会阻止“{approval.tool}”继续执行。你可以附上原因或修改建议，帮助智能体调整下一步。
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-2">
            <label className="text-sm font-medium" htmlFor={`reject-${approval.id}`}>
              反馈给智能体 <span className="text-muted-foreground">（可选）</span>
            </label>
            <textarea
              id={`reject-${approval.id}`}
              value={feedback}
              onChange={(event) => setFeedback(event.target.value)}
              rows={4}
              placeholder="说明拒绝原因或需要调整的内容…"
              className="min-h-24 max-h-48 w-full resize-y rounded-md border border-input bg-background px-3 py-2 text-sm leading-5 outline-none transition-colors focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/50"
              disabled={busy}
              autoFocus
            />
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setRejectOpen(false)} disabled={busy}>
              取消
            </Button>
            <Button
              variant="destructive"
              onClick={() => onReject(approval.id, feedback.trim())}
              disabled={busy}
            >
              {busy ? <Loader2 className="size-3.5 animate-spin motion-reduce:animate-none" /> : <Ban className="size-3.5" />}
              确认拒绝
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </section>
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
        <span className="truncate font-mono text-[11px] text-muted-foreground">{name}</span>
      </label>
      <textarea
        aria-label={toFieldLabel(name)}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        rows={value.includes("\n") ? Math.min(value.split("\n").length, 8) : 2}
        className="min-h-11 w-full resize-y rounded-md border border-input bg-background px-3 py-2 font-mono text-xs leading-5 outline-none transition-colors focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/50 disabled:bg-muted/40 disabled:text-muted-foreground"
        disabled={disabled}
      />
    </div>
  );
}
