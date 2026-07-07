"use client";

import { useEffect, useRef, useState } from "react";

const TICK_MS = 24;
const SNAP_THRESHOLD = 512;
const MIN_STEP = 3;

// Word/sentence boundary characters we prefer to snap reveal-length to,
// so text doesn't visibly cut mid-word during the reveal animation.
const BOUNDARY_RE = /[\s.,;:!?)\]}]/;

function nextBoundary(text: string, from: number, upTo: number): number {
  for (let i = upTo; i > from; i--) {
    if (BOUNDARY_RE.test(text[i - 1])) return i;
  }
  return upTo;
}

/**
 * Reveals `fullText` in small throttled steps while `live` is true, so text
 * that arrives in large chunks (our SSE/poll layer replaces full strings,
 * not tokens) still reads as a smooth stream. Falls back to showing the
 * full text immediately for historical messages, edits, or short deltas.
 */
export function usePacedText(fullText: string, live: boolean): string {
  const [shown, setShown] = useState(fullText);
  const shownRef = useRef(fullText);
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    shownRef.current = shown;
  }, [shown]);

  useEffect(() => {
    const clear = () => {
      if (timeoutRef.current !== null) {
        clearTimeout(timeoutRef.current);
        timeoutRef.current = null;
      }
    };

    const current = shownRef.current;
    const isAppend = fullText.startsWith(current);
    const remaining = isAppend ? fullText.length - current.length : 0;

    if (!live || !isAppend || remaining <= SNAP_THRESHOLD) {
      clear();
      setShown(fullText);
      return clear;
    }

    const step = () => {
      const prev = shownRef.current;
      if (!fullText.startsWith(prev) || prev.length >= fullText.length) {
        setShown(fullText);
        timeoutRef.current = null;
        return;
      }
      const left = fullText.length - prev.length;
      const chunk = Math.max(MIN_STEP, Math.round(left / 12));
      const target = nextBoundary(fullText, prev.length, Math.min(prev.length + chunk, fullText.length));
      const next = fullText.slice(0, Math.max(target, prev.length + 1));
      setShown(next);
      if (next.length < fullText.length) {
        timeoutRef.current = setTimeout(step, TICK_MS);
      } else {
        timeoutRef.current = null;
      }
    };

    clear();
    timeoutRef.current = setTimeout(step, TICK_MS);
    return clear;
  }, [fullText, live]);

  return shown;
}
