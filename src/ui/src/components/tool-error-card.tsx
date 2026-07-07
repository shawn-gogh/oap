"use client";

import { CircleAlert } from "lucide-react";
import { HighlightedCode } from "@/components/code-block";

/** Distinct red-tinted treatment for a failed tool call's error output. */
export function ToolErrorCard({ error }: { error: unknown }) {
  const text = typeof error === "string" ? error : JSON.stringify(error, null, 2);
  return (
    <div className="flex flex-col gap-2 rounded-lg border border-red-500/30 bg-red-500/5 p-3">
      <div className="flex items-center gap-1.5 text-[11px] font-medium uppercase tracking-wide text-red-600 dark:text-red-400">
        <CircleAlert className="size-3.5" />
        Error
      </div>
      <HighlightedCode code={text} lang="text" />
    </div>
  );
}
