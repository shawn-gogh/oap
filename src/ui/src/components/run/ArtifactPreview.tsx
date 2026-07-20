"use client";

import { useState } from "react";
import { Download, Paperclip } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  isTextPreviewKind,
  parseCsvTable,
  parseMinimalMarkdownLines,
  parseUriList,
  resolveArtifactPreviewKind,
  splitBoldSegments,
} from "@/lib/run/artifact-preview";
import type { RunArtifact } from "@/lib/run/types";

// Stage 5 of docs/engineering/run-surface-branch-plan.mdx: preview
// Artifacts by media type — never by `providerName` (the a2a/openapi/
// langgraph/dify/crewai fixtures each carry a different media type
// specifically to exercise this dispatch).

const FETCH_SIZE_LIMIT_BYTES = 200_000;

function resolveInlineText(artifact: RunArtifact): string | null {
  if (typeof artifact.inline === "string") return artifact.inline;
  if (artifact.inline != null) return JSON.stringify(artifact.inline, null, 2);
  return null;
}

export function ArtifactPreview({ artifact }: { artifact: RunArtifact }) {
  const kind = resolveArtifactPreviewKind(artifact.mediaType);
  const inlineText = resolveInlineText(artifact);

  const [open, setOpen] = useState(false);
  const [fetchedText, setFetchedText] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const text = inlineText ?? fetchedText;
  const canPreview = kind === "image" || kind === "pdf" || inlineText !== null || Boolean(artifact.url);

  const toggle = async () => {
    const next = !open;
    setOpen(next);
    if (!next || text !== null || !artifact.url || !isTextPreviewKind(kind)) return;

    setLoading(true);
    setError(null);
    try {
      const res = await fetch(artifact.url, { credentials: "include" });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const blob = await res.blob();
      if (blob.size > FETCH_SIZE_LIMIT_BYTES) {
        setError("文件过大，无法预览，请下载查看。");
        return;
      }
      setFetchedText(await blob.text());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="rounded-md border border-border">
      <div className="flex items-center gap-2 px-2.5 py-1.5 text-xs">
        <Paperclip className="size-3.5 shrink-0 text-muted-foreground" />
        <span className="min-w-0 truncate font-medium">{artifact.name}</span>
        <Badge variant="outline" className="shrink-0 font-mono text-[10px]">
          {artifact.mediaType}
        </Badge>
        <div className="ml-auto flex shrink-0 items-center gap-1">
          {canPreview && (
            <Button size="xs" variant="ghost" onClick={() => void toggle()}>
              {open ? "收起" : "预览"}
            </Button>
          )}
          {artifact.url && (
            <Button size="xs" variant="ghost" render={<a href={artifact.url} target="_blank" rel="noreferrer" />}>
              <Download className="size-3" />
              下载
            </Button>
          )}
        </div>
      </div>
      {open && (
        <div className="border-t border-border p-2.5 text-xs">
          {kind === "image" && artifact.url && (
            // eslint-disable-next-line @next/next/no-img-element -- previewing an arbitrary presigned Artifact URL, not a build-time asset
            <img src={artifact.url} alt={artifact.name} className="max-h-64 rounded object-contain" />
          )}
          {kind === "pdf" && artifact.url && (
            <embed src={artifact.url} type="application/pdf" className="h-64 w-full rounded" />
          )}
          {loading && <p className="text-muted-foreground">加载中…</p>}
          {error && <p className="text-destructive">{error}</p>}
          {!loading && !error && text !== null && (
            <>
              {kind === "markdown" && <MarkdownPreview text={text} />}
              {kind === "json" && <pre className="overflow-x-auto rounded bg-muted/40 p-2">{text}</pre>}
              {kind === "table" && <TablePreview text={text} />}
              {kind === "code" && (
                <pre className="overflow-x-auto rounded bg-muted/40 p-2 font-mono">{text}</pre>
              )}
              {kind === "text" && <p className="whitespace-pre-wrap">{text}</p>}
              {kind === "link-list" && <LinkListPreview text={text} />}
            </>
          )}
          {!loading && !error && text === null && artifact.url && (
            <p className="text-muted-foreground">此文件类型不支持内嵌预览，请下载查看。</p>
          )}
        </div>
      )}
    </div>
  );
}

function MarkdownPreview({ text }: { text: string }) {
  const lines = parseMinimalMarkdownLines(text);
  const HEADING_CLASS: Record<1 | 2 | 3, string> = {
    1: "text-sm font-semibold",
    2: "text-sm font-semibold",
    3: "text-xs font-semibold",
  };
  return (
    <div className="grid gap-1">
      {lines.map((line, index) =>
        line.level === 0 ? (
          <p key={index}>
            {splitBoldSegments(line.text).map((segment, segIndex) =>
              segment.bold ? <strong key={segIndex}>{segment.text}</strong> : <span key={segIndex}>{segment.text}</span>,
            )}
          </p>
        ) : (
          <p key={index} className={HEADING_CLASS[line.level]}>
            {line.text}
          </p>
        ),
      )}
    </div>
  );
}

function TablePreview({ text }: { text: string }) {
  const { headers, rows } = parseCsvTable(text);
  if (headers.length === 0) return <p className="text-muted-foreground">（空表格）</p>;
  return (
    <div className="overflow-x-auto">
      <table className="w-full border-collapse text-left">
        <thead>
          <tr>
            {headers.map((header, index) => (
              <th key={index} className="border-b border-border px-2 py-1 font-medium">
                {header}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row, rowIndex) => (
            <tr key={rowIndex}>
              {headers.map((_, cellIndex) => (
                <td key={cellIndex} className="border-b border-border/60 px-2 py-1">
                  {row[cellIndex] ?? ""}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function LinkListPreview({ text }: { text: string }) {
  const links = parseUriList(text);
  if (links.length === 0) return <p className="text-muted-foreground">（无外部引用）</p>;
  return (
    <ul className="grid gap-1">
      {links.map((link) => (
        <li key={link}>
          <a href={link} target="_blank" rel="noreferrer" className="text-primary underline underline-offset-2">
            {link}
          </a>
        </li>
      ))}
    </ul>
  );
}
