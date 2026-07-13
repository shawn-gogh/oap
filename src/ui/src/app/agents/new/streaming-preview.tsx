"use client";

export function StreamingPreview({ text }: { text: string }) {
  const lines = text.split("\n");
  const tail = lines.slice(-10).join("\n");
  return (
    <div className="w-full max-w-2xl overflow-hidden rounded-lg border border-border bg-editor-surface px-4 py-3 text-left">
      <div className="flex items-center justify-between text-[11px] text-editor-faint">
        <span className="font-mono">config.yaml</span>
        <span className="font-mono">{lines.length} lines</span>
      </div>
      <pre className="mt-2 max-h-48 overflow-hidden whitespace-pre-wrap font-mono text-xs leading-5 text-editor-accent">
        {tail}
        <span className="animate-pulse motion-reduce:animate-none">▌</span>
      </pre>
    </div>
  );
}
