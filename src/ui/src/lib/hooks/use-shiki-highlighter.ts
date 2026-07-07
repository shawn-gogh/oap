"use client";

// Curated language subset kept small on purpose — shiki/bundle/web lazily
// fetches grammars per-language on first use, so this list only bounds
// what *can* load, not what's loaded up front.
const SUPPORTED_LANGS = [
  "bash",
  "shell",
  "json",
  "python",
  "javascript",
  "typescript",
  "tsx",
  "jsx",
  "yaml",
  "markdown",
  "diff",
  "sql",
] as const;

const THEMES = ["github-light", "github-dark"] as const;

type ShikiHighlighter = import("shiki").Highlighter;

let highlighterPromise: Promise<ShikiHighlighter> | null = null;

function getHighlighter(): Promise<ShikiHighlighter> {
  if (!highlighterPromise) {
    highlighterPromise = import("shiki/bundle/web").then((mod) =>
      mod.createHighlighter({
        themes: [...THEMES],
        langs: [...SUPPORTED_LANGS],
      }),
    ) as Promise<ShikiHighlighter>;
  }
  return highlighterPromise;
}

export function normalizeLang(lang: string | undefined): string {
  const l = (lang ?? "").toLowerCase().trim();
  return (SUPPORTED_LANGS as readonly string[]).includes(l) ? l : "text";
}

/** Highlights `code` for both light/dark themes; returns null on failure (unknown lang, not yet loaded). */
export async function highlightCode(code: string, lang: string, theme: "light" | "dark"): Promise<string | null> {
  try {
    const highlighter = await getHighlighter();
    const normalized = normalizeLang(lang);
    if (normalized === "text") return null;
    return highlighter.codeToHtml(code, {
      lang: normalized,
      theme: theme === "dark" ? "github-dark" : "github-light",
    });
  } catch {
    return null;
  }
}
