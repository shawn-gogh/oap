"use client";

import { useEffect, useState } from "react";
import { RotateCcw } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Textarea } from "@/components/ui/textarea";
import { StatusDot } from "@/components/status-dot";
import { fixtureRunTransport } from "@/lib/run/fixture-client";
import { applyRunEvent } from "@/lib/run/apply-event";
import type { RunSnapshotV1 } from "@/lib/run/types";
import type { RunTransport } from "@/lib/run/transport";
import { buildRunView } from "./run-view-model";
import { SchemaFieldsForm } from "./SchemaFieldsForm";
import { InvocationTimeline } from "./InvocationTimeline";
import { PendingInputCard } from "./PendingInputCard";
import { ArtifactPreview } from "./ArtifactPreview";

// Provider-neutral Run container (Stage 2 of docs/engineering/run-surface-branch-plan.mdx).
// Every section reads only RunSnapshotV1 fields — `providerName` is shown as
// a metadata badge and never selects behavior. Step-level detail lives in
// InvocationTimeline (Stage 4), structured input-request fields in
// PendingInputCard, and Artifact previews in ArtifactPreview (Stage 5); see
// the module's fixture index for representative snapshots this shell must
// already handle.
//
// `transport` defaults to the fixture transport so every existing caller
// (the /dev/run-shell/ fixture demos, Stage 1-3's tests) is unaffected;
// pass `real-client.ts`'s `createRealRunTransport(sessionId)` to point this
// same component at a live backend Run (Stage 7).

export function RunShell({
  runId,
  transport = fixtureRunTransport,
}: {
  runId: string;
  transport?: RunTransport;
}) {
  const [activeRunId, setActiveRunId] = useState(runId);
  const [snapshot, setSnapshot] = useState<RunSnapshotV1 | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<"cancel" | "retry" | "input" | "approval" | null>(null);
  const [inputValues, setInputValues] = useState<Record<string, string>>({});
  const [rejectFeedback, setRejectFeedback] = useState("");

  useEffect(() => {
    setActiveRunId(runId);
  }, [runId]);

  useEffect(() => {
    let cancelled = false;
    let unsubscribe: (() => void) | undefined;
    setSnapshot(null);
    setError(null);
    transport
      .getRunSnapshot(activeRunId)
      .then((next) => {
        if (cancelled) return;
        setSnapshot(next);
        // Subscribing here (rather than a separate effect keyed on
        // `snapshot`) is deliberate: a second effect depending on `[runId]`
        // alone would only ever see `snapshot === null` on its first (and
        // only, since `runId` hasn't changed) run, silently never
        // subscribing. Chaining it onto the same async load ties the
        // subscription's lifetime to this effect's cleanup instead.
        unsubscribe = transport.subscribeRunEvents(activeRunId, next.lastEventSeq, (event) => {
          setSnapshot((current) => (current ? applyRunEvent(current, event) : current));
        });
      })
      .catch((e) => {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      });
    return () => {
      cancelled = true;
      unsubscribe?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- transport identity isn't expected to change independently of runId
  }, [activeRunId]);

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
      const next = await command();
      setSnapshot(next);
      if (next.runId !== activeRunId) setActiveRunId(next.runId);
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
        await transport.decideRunApproval({
          runId: snapshot.runId,
          approvalId: snapshot.pendingApproval.id,
          decision,
          feedback: decision === "rejected" ? rejectFeedback || undefined : undefined,
        }),
      );
      setRejectFeedback("");
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
        await transport.submitRunInput({
          runId: snapshot.runId,
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
                onClick={() =>
                  void runAction("cancel", () =>
                    transport.cancelRun({ runId: snapshot.runId }),
                  )
                }
              >
                {busy === "cancel" ? "取消中…" : "取消"}
              </Button>
            )}
            {view.canRetry && (
              <Button
                size="sm"
                variant="outline"
                disabled={busy !== null}
                onClick={() =>
                  void runAction("retry", () =>
                    transport.retryRun({
                      runId: snapshot.runId,
                      requestId: crypto.randomUUID(),
                    }),
                  )
                }
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
        <SchemaFieldsForm
          schema={snapshot.interactionProfile.inputSchema}
          value={view.inputSnapshot}
          readOnly
        />
      </Card>

      {view.invocations.length > 0 && (
        <Card className="grid gap-2 p-4">
          <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
            执行时间线
          </h3>
          <InvocationTimeline invocations={view.invocations} />
        </Card>
      )}

      {view.operations.length > 0 && (
        <Card className="grid gap-2 p-4">
          <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
            操作
          </h3>
          <div className="grid gap-2">
            {view.operations.map((operation) => (
              <div
                key={operation.id}
                className="flex items-start justify-between gap-3 rounded-md border px-3 py-2"
              >
                <div className="min-w-0">
                  <p className="truncate text-sm font-medium">{operation.type}</p>
                  {operation.error != null && (
                    <p className="mt-1 text-xs text-destructive">
                      {typeof operation.error === "string"
                        ? operation.error
                        : JSON.stringify(operation.error)}
                    </p>
                  )}
                </div>
                <span className="shrink-0 text-xs text-muted-foreground">
                  {operation.status}
                </span>
              </div>
            ))}
          </div>
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
          <div className="grid gap-1">
            <label htmlFor="run-reject-feedback" className="text-xs text-muted-foreground">
              拒绝理由（可选，仅拒绝时提交）
            </label>
            <Textarea
              id="run-reject-feedback"
              value={rejectFeedback}
              onChange={(event) => setRejectFeedback(event.target.value)}
              rows={2}
            />
          </div>
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
        <PendingInputCard
          request={snapshot.pendingInputRequest}
          values={inputValues}
          onChange={(fieldId, value) =>
            setInputValues((current) => ({ ...current, [fieldId]: value }))
          }
          onSubmit={() => void submitInput()}
          busy={busy === "input"}
        />
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
            <div className="grid gap-1.5">
              {snapshot.artifacts.map((artifact) => (
                <ArtifactPreview key={artifact.id} artifact={artifact} />
              ))}
            </div>
          )}
        </Card>
      )}
    </div>
  );
}
