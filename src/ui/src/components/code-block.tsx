"use client";

import { useEffect, useState } from "react";
import { useTheme } from "next-themes";
import { highlightCode } from "@/lib/hooks/use-shiki-highlighter";
import { CopyButton } from "@/components/copy-button";

/** Renders a highlighted code block, falling back to plain <pre> until Shiki loads or for unknown languages. */
export function HighlightedCode({ code, lang, className }: { code: string; lang?: string; className?: string }) {
  const { resolvedTheme } = useTheme();
  const isDark = resolvedTheme === "dark";
  const [html, setHtml] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setHtml(null);
    highlightCode(code, lang ?? "text", isDark ? "dark" : "light").then((result) => {
      if (!cancelled) setHtml(result);
    });
    return () => {
      cancelled = true;
    };
  }, [code, lang, isDark]);

  return (
    <div className={`group/code relative ${className ?? ""}`}>
      <CopyButton
        text={code}
        className="absolute right-2 top-2 z-10 bg-background/80 opacity-0 backdrop-blur transition-opacity group-hover/code:opacity-100"
      />
      {html ? (
        <div className="shiki-container overflow-auto rounded-md text-[13px] [&_pre]:p-3" dangerouslySetInnerHTML={{ __html: html }} />
      ) : (
        <pre className="mono max-h-96 overflow-auto rounded-md border border-border bg-background p-3 text-[13px] text-foreground whitespace-pre-wrap break-words">
          {code}
        </pre>
      )}
    </div>
  );
}

// ReactMarkdown `code` component override: fenced blocks (with a language
// class) get syntax highlighting; inline code stays a plain <code> tag.
export function MarkdownCodeBlock(props: React.ComponentPropsWithoutRef<"code"> & { node?: unknown }) {
  const { className, children, node: _node, ...rest } = props;
  const match = /language-(\w+)/.exec(className ?? "");
  const isBlock = Boolean(match);
  const text = String(children ?? "").replace(/\n$/, "");

  if (!isBlock) {
    return (
      <code className={`mono rounded bg-muted px-1 py-0.5 text-[0.9em] ${className ?? ""}`} {...rest}>
        {children}
      </code>
    );
  }

  return <HighlightedCode code={text} lang={match?.[1]} className="my-2" />;
}

// ReactMarkdown wraps block code in <pre><code>; since MarkdownCodeBlock
// already renders its own container for fenced blocks, unwrap <pre> so we
// don't double up on <pre> elements.
export function MarkdownPre(props: React.ComponentPropsWithoutRef<"pre"> & { node?: unknown }) {
  const { children } = props;
  return <>{children}</>;
}
