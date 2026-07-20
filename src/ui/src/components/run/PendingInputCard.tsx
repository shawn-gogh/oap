"use client";

import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger } from "@/components/ui/select";
import type { RunInputRequest } from "@/lib/run/types";

// Stage 5 of docs/engineering/run-surface-branch-plan.mdx: dispatch each
// pending-input field by its declared `kind` instead of always rendering a
// plain text box, mirroring how SchemaFieldsForm.tsx dispatches Stage 3's
// JSON-Schema fields by kind.
//
// `auth` is deliberately never a data-entry field — this app's safety rules
// block collecting credentials in-app, and the real backend's shape for an
// auth-kind request isn't confirmed yet (same known-gap category as
// adapt-backend.ts's single-generic-field simplification for waiting_input
// turns). `authUrl` is an additive, optional field on the frontend contract
// (types.ts is documented additive-only); when present it renders as a link
// out to complete authorization elsewhere, otherwise just the prompt shows.

export function PendingInputCard({
  request,
  values,
  onChange,
  onSubmit,
  busy,
}: {
  request: RunInputRequest;
  values: Record<string, string>;
  onChange: (fieldId: string, value: string) => void;
  onSubmit: () => void;
  busy: boolean;
}) {
  const requiresSubmit = request.fields.some((field) => field.kind !== "auth");

  return (
    <Card className="grid gap-2 border-amber-500/40 p-4">
      <h3 className="text-xs font-semibold uppercase tracking-wider text-amber-700 dark:text-amber-400">
        需要补充输入
      </h3>
      <p className="text-sm">{request.prompt}</p>
      <div className="grid gap-2 sm:grid-cols-2">
        {request.fields.map((field) => (
          <div key={field.id} className="grid gap-1">
            <label htmlFor={`run-input-${field.id}`} className="text-xs text-muted-foreground">
              {field.label}
              {field.required && <span className="text-destructive"> *</span>}
            </label>
            {field.kind === "choice" ? (
              <Select value={values[field.id] ?? ""} onValueChange={(next) => onChange(field.id, next ?? "")}>
                <SelectTrigger id={`run-input-${field.id}`} className="h-8 w-full text-xs">
                  {values[field.id] || "请选择"}
                </SelectTrigger>
                <SelectContent>
                  {field.choices?.map((choice) => (
                    <SelectItem key={choice} value={choice}>
                      {choice}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            ) : field.kind === "auth" ? (
              field.authUrl ? (
                <Button
                  size="sm"
                  variant="outline"
                  render={<a href={field.authUrl} target="_blank" rel="noreferrer" />}
                >
                  前往授权
                </Button>
              ) : (
                <p className="text-xs text-muted-foreground">此步骤需要额外授权，请联系管理员完成。</p>
              )
            ) : (
              <Input
                id={`run-input-${field.id}`}
                value={values[field.id] ?? ""}
                onChange={(event) => onChange(field.id, event.target.value)}
                placeholder={field.kind === "file" ? "文件或 Artifact 引用" : undefined}
              />
            )}
          </div>
        ))}
      </div>
      {requiresSubmit && (
        <div>
          <Button size="sm" disabled={busy} onClick={onSubmit}>
            {busy ? "提交中…" : "提交"}
          </Button>
        </div>
      )}
    </Card>
  );
}
