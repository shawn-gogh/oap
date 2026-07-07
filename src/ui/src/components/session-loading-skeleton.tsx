function SkeletonLine({ width }: { width: string }) {
  return <div className={`h-3 rounded-full bg-muted-foreground/15 ${width}`} />;
}

function SkeletonUserBubble() {
  return (
    <div className="flex justify-end">
      <div className="flex w-full max-w-[min(560px,70%)] flex-col gap-2 rounded-[18px] border border-border/60 bg-muted/40 px-5 py-3">
        <SkeletonLine width="w-3/4" />
        <SkeletonLine width="w-1/2" />
      </div>
    </div>
  );
}

function SkeletonAssistantBlock() {
  return (
    <div className="flex max-w-[720px] flex-col gap-2.5">
      <SkeletonLine width="w-full" />
      <SkeletonLine width="w-11/12" />
      <SkeletonLine width="w-2/3" />
    </div>
  );
}

/**
 * Shown in place of the message list while a session's history is still
 * loading. Mimics the eventual layout (alternating user/assistant blocks)
 * so the transition to real content doesn't jump, and gives a clearer
 * "this is loading, not empty" signal than a bare "Loading…" label.
 */
export function SessionLoadingSkeleton() {
  return (
    <div aria-live="polite" aria-busy="true" className="flex flex-col gap-6 py-1 animate-pulse motion-reduce:animate-none">
      <SkeletonUserBubble />
      <SkeletonAssistantBlock />
      <SkeletonUserBubble />
      <SkeletonAssistantBlock />
    </div>
  );
}
