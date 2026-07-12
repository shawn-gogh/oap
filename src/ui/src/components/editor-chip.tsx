import { cn } from "@/lib/utils";

/** Compact mono chip for the dark editor panels (tool ids, token lists).
 *  Keeps the border/background/typography of these chips in one place. */
export function EditorChip({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <span
      className={cn(
        "rounded border border-white/10 bg-white/5 px-1.5 py-0.5 font-mono text-[11px] text-editor-muted",
        className,
      )}
    >
      {children}
    </span>
  );
}
