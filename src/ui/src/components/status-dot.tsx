import { cn } from "@/lib/utils";

export type StatusDotTone = "success" | "warning" | "idle" | "error";

const TONE_CLASS: Record<StatusDotTone, string> = {
  success: "bg-emerald-500",
  warning: "bg-amber-500",
  idle: "bg-muted-foreground/40",
  error: "bg-destructive",
};

/** Small status indicator dot. Always pair it with a visible or accessible
 *  label — color alone must never be the only signal. */
export function StatusDot({
  tone,
  label,
  className,
}: {
  tone: StatusDotTone;
  /** Accessible description, e.g. "运行中". Rendered for screen readers. */
  label: string;
  className?: string;
}) {
  return (
    <span
      className={cn("inline-block size-1.5 shrink-0 rounded-full", TONE_CLASS[tone], className)}
      role="img"
      aria-label={label}
      title={label}
    />
  );
}
