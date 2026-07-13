import type { LucideIcon } from "lucide-react";
import { cn } from "@/lib/utils";

/** Standard empty-state block: icon, one-line message, optional hint and
 *  action. Use this instead of ad-hoc "No X yet." paragraphs so empty screens
 *  read consistently across pages. */
export function EmptyState({
  icon: Icon,
  title,
  hint,
  action,
  className,
}: {
  icon: LucideIcon;
  title: string;
  hint?: string;
  action?: React.ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "rounded-lg border border-dashed border-border bg-card/60 px-4 py-6 text-center",
        className,
      )}
    >
      <Icon className="mx-auto size-6 text-muted-foreground" />
      <p className="mt-2 text-sm text-muted-foreground">{title}</p>
      {hint && <p className="mt-1 text-xs text-muted-foreground">{hint}</p>}
      {action && <div className="mt-3 flex justify-center">{action}</div>}
    </div>
  );
}
