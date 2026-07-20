"use client";

import { useState } from "react";

import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { createRun } from "@/lib/run/fixture-client";
import { describeSchema, validateValue } from "@/lib/run/schema-form";
import type { JsonSchema } from "@/lib/run/types";
import { SchemaFieldsForm } from "./SchemaFieldsForm";

// Stage 3's pre-submission input form. Validation and field rendering are
// entirely delegated to lib/run/schema-form.ts + SchemaFieldsForm — this
// component only owns the draft value, submit state, and the
// createRun round trip. No agent- or provider-specific branching.

interface RunInputFormProps {
  agentId: string;
  agentName: string;
  schema: JsonSchema | null;
  onCreated: (runId: string) => void;
}

export function RunInputForm({ agentId, agentName, schema, onCreated }: RunInputFormProps) {
  const [value, setValue] = useState<unknown>({});
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);

  const submit = async () => {
    const fields = describeSchema(schema);
    const nextErrors = fields ? validateValue(fields, value) : {};
    setErrors(nextErrors);
    if (Object.keys(nextErrors).length > 0) return;

    setSubmitting(true);
    setSubmitError(null);
    try {
      const created = await createRun({ agentId, input: value });
      onCreated(created.runId);
    } catch (e) {
      setSubmitError(e instanceof Error ? e.message : String(e));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Card className="grid gap-3 p-4">
      <h3 className="text-sm font-semibold">向「{agentName}」提交输入</h3>
      <SchemaFieldsForm schema={schema} value={value} readOnly={false} errors={errors} onChange={setValue} />
      {submitError && <p className="text-sm text-destructive">{submitError}</p>}
      <div>
        <Button size="sm" disabled={submitting} onClick={() => void submit()}>
          {submitting ? "提交中…" : "开始运行"}
        </Button>
      </div>
    </Card>
  );
}
