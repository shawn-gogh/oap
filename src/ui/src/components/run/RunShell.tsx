"use client";

import { useEffect, useState } from "react";
import { CheckCircle2, Loader2, Paperclip, RotateCcw, XCircle } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { StatusDot } from "@/components/status-dot";
import { Input } from "@/components/ui/input";
import {
  cancelRun,
  decideRunApproval,
  getRunSnapshot,
  retryRun,
  submitRunInput,
  subscribeRunEvents,
} from "@/lib/run/fixture-client";
import { applyRunEvent } from "@/lib/run/apply-event";
import type { RunSnapshotV1 } from "@/lib/run/types";
import { buildRunView } from "./run-view-model";

// Provider-neutral Run container (Stage 2 of docs/engineering/run-surface-branch-plan.mdx).
// Every section reads only RunSnapshotV1 fields — `providerName` is shown as
// a metadata badge and never selects behavior. Structured-input rendering
// (Stage 3), step-level detail (Stage 4), and rich Artifact previews
// (Stage 5) are intentionally out of scope here; see the module's fixture
// index for representative snapshots this shell must already handle.

export function RunShell({ runId }: { runId: string }) {
  const [snapshot, setSnapshot] = useState<RunSnapshotV1 | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<"cancel" | "retry" | "input" | "approval" | null>(null);
  const [inputValues, setInputValues] = useState<Record<string, string>>({});

  useEffect(() => {
    let cancelled = false;
    setSnapshot(null);
    setError(null);
    getRunSnapshot(runId)
      .then((next) => {
        if (!cancelled) setSnapshot(next);
      })
      .catch((e) => {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [runId]);

  useEffect(() => {
    if (!snapshot) return;
    const unsubscribe = subscribeRunEvents(runId, snapshot.lastEventSeq, (event) => {
      setSnapshot((current) => (current ? applyRunEvent(current, event) : current));
    });
    return unsubscribe;
    // eslint-disable-next-line react-hooks/exhaustive-deps -- resubscribes only when the run identity changes, not on every snapshot update
  }, [runId]);

  if (error) {
    return (
      <Card className="p-4 text-sm text-destructive">运行加载失败：{error}</Card>
    );
  }

  if (!snapshot) {
    return (
      <Card className="grid gap-3 p-4">
        <div className="h-5 w-48 animate-pulse rounded bg-muted" />
        <div className="h-4 w-full animate-pulse rounded bg-muted" />
        <div className="h-24 w-full animate-pulse rounded bg-muted" />
      </Card>
    );
  }

  const view = buildRunView(snapshot);

  const runAction = async (kind: "cancel" | "retry", command: () => Promise<RunSnapshotV1>) => {
    setBusy(kind);
    try {
      setSnapshot(await command());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  };

  const resolveApproval = async (decision: "accepted" | "rejected") => {
    if (!snapshot.pendingApproval) return;
    setBusy("approval");
    try {
      setSnapshot(
        await decideRunApproval({ runId, approvalId: snapshot.pendingApproval.id, decision }),
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  };

  const submitInput = async () => {
    if (!snapshot.pendingInputRequest) return;
    setBusy("input");
    try {
      setSnapshot(
        await submitRunInput({
          runId,
          requestId: snapshot.pendingInputRequest.id,
          values: inputValues,
        }),
      );
      setInputValues({});
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="grid gap-3">
      <Card className="grid gap-3 p-4">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <h2 className="truncate text-base font-semibold">{view.title}</h2>
            <p className="mt-1 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
              <StatusDot tone={view.statusTone} label={view.statusLabel} />
              {view.statusLabel}
              <span aria-hidden>·</span>
              {view.triggerLabel}
              {view.providerLabel && (
                <Badge variant="outline" className="font-mono text-[10px]">
                  {view.providerLabel}
                </Badge>
              )}
            </p>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            {view.canCancel && (
              <Button
                size="sm"
                variant="outline"
                disabled={busy !== null}
                onClick={() => void runAction("cancel", () => cancelRun({ runId }))}
              >
                {busy === "cancel" ? "取消中…" : "取消"}
              </Button>
            )}
            {view.canRetry && (
              <Button
                size="sm"
                variant="outline"
                disabled={busy !== null}
                onClick={() => void runAction("retry", () => retryRun({ runId }))}
              >
                <RotateCcw className="size-3.5" />
                {busy === "retry" ? "重试中…" : "重试"}
              </Button>
            )}
          </div>
        </div>

        {view.progress && (
          <div className="grid gap-1">
            <div className="flex items-center justify-between text-xs text-muted-foreground">
              <span>{view.progress.label}</span>
              {view.progress.total != null && (
                <span>
                  {view.progress.current}/{view.progress.total}
                </span>
              )}
            </div>
            <div className="h-1.5 overflow-hidden rounded-full bg-muted">
              <div
                className="h-full rounded-full bg-primary transition-all"
                style={{
                  width: view.progress.total
                    ? `${Math.min(100, (view.progress.current / view.progress.total) * 100)}%`
                    : "100%",
                }}
              />
            </div>
          </div>
        )}
      </Card>

      <Card className="grid gap-2 p-4">
        <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
          输入
        </h3>
        <pre className="overflow-x-auto rounded-md bg-muted/40 p-2 text-xs">
          {JSON.stringify(view.inputSnapshot, null, 2)}
        </pre>
      </Card>

      {view.invocations.length > 0 && (
        <Card className="grid gap-2 p-4">
          <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
            执行时间线
          </h3>
          <ol className="grid gap-2">
            {view.invocations.map((invocation) => (
              <li
                key={invocation.id}
                className="flex items-start gap-2 rounded-md border border-border px-3 py-2 text-sm"
              >
                {invocation.status === "completed" ? (
                  <CheckCircle2 className="mt-0.5 size-4 shrink-0 text-emerald-600 dark:text-emerald-400" />
                ) : invocation.status === "failed" ? (
                  <XCircle className="mt-0.5 size-4 shrink-0 text-destructive" />
                ) : (
                  <Loader2 className="mt-0.5 size-4 shrink-0 animate-spin text-muted-foreground" />
                )}
                <div className="min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="font-medium">{invocation.label}</span>
                    <Badge variant="outline" className="text-[10px]">
                      {invocation.role === "agent" ? "智能体" : "工具"}
                    </Badge>
                  </div>
                  {invocation.summary && (
                    <p className="mt-0.5 text-xs text-muted-foreground">{invocation.summary}</p>
                  )}
                </div>
              </li>
            ))}
          </ol>
        </Card>
      )}

      {snapshot.pendingApproval && (
        <Card className="grid gap-2 border-amber-500/40 p-4">
          <h3 className="text-xs font-semibold uppercase tracking-wider text-amber-700 dark:text-amber-400">
            待处理审批
          </h3>
          <p className="text-sm font-medium">{snapshot.pendingApproval.title}</p>
          {snapshot.pendingApproval.body && (
            <p className="text-xs text-muted-foreground">{snapshot.pendingApproval.body}</p>
          )}
          <div className="flex gap-2">
            <Button size="sm" disabled={busy !== null} onClick={() => void resolveApproval("accepted")}>
              批准
            </Button>
            <Button
              size="sm"
              variant="outline"
              disabled={busy !== null}
              onClick={() => void resolveApproval("rejected")}
            >
              拒绝
            </Button>
          </div>
        </Card>
      )}

      {snapshot.pendingInputRequest && (
        <Card className="grid gap-2 border-amber-500/40 p-4">
          <h3 className="text-xs font-semibold uppercase tracking-wider text-amber-700 dark:text-amber-400">
            需要补充输入
          </h3>
          <p className="text-sm">{snapshot.pendingInputRequest.prompt}</p>
          <div className="grid gap-2 sm:grid-cols-2">
            {snapshot.pendingInputRequest.fields.map((field) => (
              <div key={field.id} className="grid gap-1">
                <label htmlFor={`run-input-${field.id}`} className="text-xs text-muted-foreground">
                  {field.label}
                  {field.required && <span className="text-destructive"> *</span>}
                </label>
                <Input
                  id={`run-input-${field.id}`}
                  value={inputValues[field.id] ?? ""}
                  onChange={(event) =>
                    setInputValues((current) => ({ ...current, [field.id]: event.target.value }))
                  }
                  placeholder={field.choices?.join(" / ")}
                />
              </div>
            ))}
          </div>
          <div>
            <Button size="sm" disabled={busy !== null} onClick={() => void submitInput()}>
              {busy === "input" ? "提交中…" : "提交"}
            </Button>
          </div>
        </Card>
      )}

      {snapshot.error && (
        <Card className="border-destructive/40 p-4 text-sm text-destructive">
          {snapshot.error.message}
        </Card>
      )}

      {(snapshot.result || snapshot.artifacts.length > 0) && (
        <Card className="grid gap-3 p-4">
          <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
            结果
          </h3>
          {snapshot.result?.kind === "text" && snapshot.result.text && (
            <p className="text-sm">{snapshot.result.text}</p>
          )}
          {snapshot.result?.kind === "json" && (
            <pre className="overflow-x-auto rounded-md bg-muted/40 p-2 text-xs">
              {JSON.stringify(snapshot.result.json, null, 2)}
            </pre>
          )}
          {snapshot.artifacts.length > 0 && (
            <ul className="grid gap-1.5">
              {snapshot.artifacts.map((artifact) => (
                <li
                  key={artifact.id}
                  className="flex items-center gap-2 rounded-md border border-border px-2.5 py-1.5 text-xs"
                >
                  <Paperclip className="size-3.5 shrink-0 text-muted-foreground" />
                  <span className="min-w-0 truncate font-medium">{artifact.name}</span>
                  <Badge variant="outline" className="ml-auto shrink-0 font-mono text-[10px]">
                    {artifact.mediaType}
                  </Badge>
                </li>
              ))}
            </ul>
          )}
        </Card>
      )}
    </div>
  );
}
