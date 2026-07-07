"use client";

import { useEffect, useRef } from "react";

/** Grows a textarea's height to fit its content up to `maxHeightPx`, then scrolls internally. */
export function useAutosizeTextarea(value: string, maxHeightPx = 240) {
  const ref = useRef<HTMLTextAreaElement | null>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.style.height = "auto";
    const next = Math.min(el.scrollHeight, maxHeightPx);
    el.style.height = `${next}px`;
    el.style.overflowY = el.scrollHeight > maxHeightPx ? "auto" : "hidden";
  }, [value, maxHeightPx]);

  return ref;
}
