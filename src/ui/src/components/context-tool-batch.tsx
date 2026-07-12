"use client";

import { useState } from "react";
import { ChevronDown, Loader2 } from "lucide-react";
import type { HarnessMessagePart } from "@/lib/types";
import { ToolBlock, toolDescriptor, toolLabel } from "@/components/message-block";

type ToolPart = Extract<HarnessMessagePart, { type: "tool" }>;

function summarize(parts: ToolPart[]): string {
  const counts = new Map<string, number>();
  for (const part of parts) {
    const label = toolLabel(part.tool);
    counts.set(label, (counts.get(label) ?? 0) + 1);
  }
  return Array.from(counts.entries())
    .map(([label, count]) => `${label}${count > 1 ? ` ×${count}` : ""}`)
    .join(", ");
}

/** Collapsed summary row for a run of consecutive read-only "context" tool calls. */
export function ContextToolBatch({ parts }: { parts: ToolPart[] }) {
  const [open, setOpen] = useState(false);
  const running = parts.some((p) => p.state?.status === "running");
  const firstDesc = toolDescriptor(parts[0].tool, parts[0].state?.input);

  return (
    <div className="max-w-[920px] text-[13px]">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        className="inline-flex max-w-full min-w-0 items-center gap-2 rounded-lg px-2.5 py-2 text-left text-muted-foreground transition-colors hover:bg-muted/55"
      >
        {running ? (
          <Loader2 className="size-3.5 shrink-0 animate-spin motion-reduce:animate-none" />
        ) : (
          <span className="size-3.5 shrink-0" />
        )}
        <span className="min-w-0 truncate text-[13px]">
          {summarize(parts)}
          {firstDesc && !open && <span className="mono ml-1.5 text-xs text-muted-foreground/70">{firstDesc}</span>}
        </span>
        <ChevronDown className={`size-3.5 shrink-0 transition-transform ${open ? "" : "-rotate-90"}`} />
      </button>

      {open && (
        <div className="ml-6 flex flex-col gap-1">
          {parts.map((part, index) => (
            <ToolBlock key={`${part.id ?? "tool"}-${index}`} part={part} />
          ))}
        </div>
      )}
    </div>
  );
}
