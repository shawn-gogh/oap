"use client";

import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger } from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { describeSchema, type SchemaField } from "@/lib/run/schema-form";
import type { JsonSchema } from "@/lib/run/types";

// Stage 3 shell: one field renderer, two modes (edit / read-only), so the
// pre-submission input form and RunShell's post-submission display can
// never drift apart. See the Stage 3 plan's "Design decision" section.

function linesToList(value: string): string[] {
  return value
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
}

function get(value: unknown, key: string): unknown {
  if (!value || typeof value !== "object" || Array.isArray(value)) return undefined;
  return (value as Record<string, unknown>)[key];
}

function set(value: unknown, key: string, next: unknown): Record<string, unknown> {
  const base = value && typeof value === "object" && !Array.isArray(value) ? (value as Record<string, unknown>) : {};
  return { ...base, [key]: next };
}

interface SchemaFieldsFormProps {
  schema: JsonSchema | null;
  value: unknown;
  readOnly: boolean;
  onChange?: (value: unknown) => void;
  errors?: Record<string, string>;
}

export function SchemaFieldsForm({ schema, value, readOnly, onChange, errors }: SchemaFieldsFormProps) {
  const fields = describeSchema(schema);

  if (!fields) {
    return (
      <JsonFallback
        value={value}
        readOnly={readOnly}
        onChange={onChange}
        error={errors?.[""]}
      />
    );
  }

  return (
    <div className="grid gap-3 sm:grid-cols-2">
      {fields.map((field) => (
        <FieldRow
          key={field.path.join(".")}
          field={field}
          value={get(value, field.key)}
          readOnly={readOnly}
          error={errors?.[field.path.join(".")]}
          onChange={
            onChange && ((next) => onChange(set(value, field.key, next)))
          }
        />
      ))}
    </div>
  );
}

function JsonFallback({
  value,
  readOnly,
  onChange,
  error,
}: {
  value: unknown;
  readOnly: boolean;
  onChange?: (value: unknown) => void;
  error?: string;
}) {
  if (readOnly) {
    return (
      <pre className="overflow-x-auto rounded-md bg-muted/40 p-2 text-xs">
        {JSON.stringify(value, null, 2)}
      </pre>
    );
  }
  return (
    <div className="grid gap-1">
      <Textarea
        className="min-h-40 font-mono text-xs"
        defaultValue={JSON.stringify(value ?? {}, null, 2)}
        onChange={(event) => {
          try {
            onChange?.(JSON.parse(event.target.value));
          } catch {
            // Leave the last valid value in place; the raw text stays
            // visible in the textarea so the user doesn't lose their edit.
          }
        }}
      />
      <p className="text-xs text-muted-foreground">
        该智能体的输入结构无法用表单渲染（例如包含 oneOf/anyOf 或多层嵌套），请直接编辑 JSON。
      </p>
      {error && <p className="text-xs text-destructive">{error}</p>}
    </div>
  );
}

function FieldRow({
  field,
  value,
  readOnly,
  error,
  onChange,
}: {
  field: SchemaField;
  value: unknown;
  readOnly: boolean;
  error?: string;
  onChange?: (value: unknown) => void;
}) {
  const fieldId = `run-field-${field.path.join("-")}`;
  const wide = field.kind === "object" || field.kind === "array" || field.kind === "string";

  return (
    <div className={`grid gap-1 ${wide ? "sm:col-span-2" : ""}`}>
      <Label htmlFor={fieldId} className="text-xs text-muted-foreground">
        {field.label}
        {field.required && !readOnly && <span className="text-destructive"> *</span>}
      </Label>
      <FieldInput fieldId={fieldId} field={field} value={value} readOnly={readOnly} onChange={onChange} />
      {error && !readOnly && <p className="text-xs text-destructive">{error}</p>}
    </div>
  );
}

function FieldInput({
  fieldId,
  field,
  value,
  readOnly,
  onChange,
}: {
  fieldId: string;
  field: SchemaField;
  value: unknown;
  readOnly: boolean;
  onChange?: (value: unknown) => void;
}) {
  if (readOnly) {
    return <p className="min-h-8 py-1 text-sm">{formatReadOnlyValue(field, value)}</p>;
  }

  switch (field.kind) {
    case "string":
    case "file":
      return (
        <Input
          id={fieldId}
          value={typeof value === "string" ? value : ""}
          onChange={(event) => onChange?.(event.target.value)}
          placeholder={field.kind === "file" ? "文件或 Artifact 引用" : undefined}
        />
      );
    case "number":
      return (
        <Input
          id={fieldId}
          type="number"
          value={typeof value === "number" ? String(value) : ""}
          onChange={(event) => {
            const next = event.target.value;
            onChange?.(next === "" ? undefined : Number(next));
          }}
        />
      );
    case "boolean":
      return (
        <label className="flex h-8 items-center gap-2 text-sm">
          <input
            id={fieldId}
            type="checkbox"
            checked={value === true}
            onChange={(event) => onChange?.(event.target.checked)}
          />
          启用
        </label>
      );
    case "enum":
      return (
        <Select value={typeof value === "string" ? value : ""} onValueChange={(next) => onChange?.(next)}>
          <SelectTrigger id={fieldId} className="h-8 w-full text-xs">
            {typeof value === "string" && value ? value : "请选择"}
          </SelectTrigger>
          <SelectContent>
            {field.enumValues?.map((option) => (
              <SelectItem key={option} value={option}>
                {option}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      );
    case "array":
      return (
        <Textarea
          id={fieldId}
          className="min-h-16 text-sm"
          value={Array.isArray(value) ? value.join("\n") : ""}
          onChange={(event) => {
            const lines = linesToList(event.target.value);
            onChange?.(field.itemKind === "number" ? lines.map(Number) : lines);
          }}
          placeholder="每行一项"
        />
      );
    case "object":
      return (
        <div className="grid gap-2 rounded-md border border-border p-2 sm:grid-cols-2">
          {field.properties?.map((child) => (
            <FieldRow
              key={child.path.join(".")}
              field={child}
              value={get(value, child.key)}
              readOnly={false}
              onChange={(next) => onChange?.(set(value, child.key, next))}
            />
          ))}
        </div>
      );
    default:
      return null;
  }
}

function formatReadOnlyValue(field: SchemaField, value: unknown): string {
  if (value === undefined || value === null || value === "") return "—";
  if (field.kind === "boolean") return value ? "是" : "否";
  if (field.kind === "array" && Array.isArray(value)) return value.join("、");
  if (field.kind === "object") return JSON.stringify(value);
  return String(value);
}
