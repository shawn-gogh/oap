"use client";

import { ArrowDown } from "lucide-react";

export function JumpToBottomButton({ onClick }: { onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      title="Jump to latest"
      aria-label="Jump to latest"
      className="absolute bottom-4 left-1/2 z-10 flex size-8 -translate-x-1/2 items-center justify-center rounded-full border border-border bg-background text-foreground shadow-md transition-colors hover:bg-muted"
    >
      <ArrowDown className="size-4" />
    </button>
  );
}
