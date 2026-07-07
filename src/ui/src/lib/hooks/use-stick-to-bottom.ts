"use client";

import { useCallback, useEffect, useRef, useState } from "react";

const BOTTOM_THRESHOLD_PX = 120;
// Ignore scroll events for a moment after we scroll programmatically, so our
// own auto-scroll doesn't get mistaken for the user scrolling away.
const PROGRAMMATIC_SCROLL_GUARD_MS = 150;

export interface StickToBottomResult {
  scrollRef: (el: HTMLDivElement | null) => void;
  contentRef: (el: HTMLDivElement | null) => void;
  onScroll: () => void;
  isPinned: boolean;
  jumpToBottom: () => void;
}

/**
 * Keeps a scroll container pinned to the bottom while `active` (e.g. a turn
 * is streaming in), but stops following as soon as the user scrolls up, and
 * lets them jump back with `jumpToBottom()`.
 */
export function useStickToBottom(active: boolean): StickToBottomResult {
  const elRef = useRef<HTMLDivElement | null>(null);
  const contentElRef = useRef<HTMLDivElement | null>(null);
  const userScrolledRef = useRef(false);
  const programmaticUntilRef = useRef(0);
  const [isPinned, setIsPinned] = useState(true);

  const scrollToBottom = useCallback(() => {
    const el = elRef.current;
    if (!el) return;
    programmaticUntilRef.current = Date.now() + PROGRAMMATIC_SCROLL_GUARD_MS;
    el.scrollTop = el.scrollHeight;
  }, []);

  const jumpToBottom = useCallback(() => {
    userScrolledRef.current = false;
    setIsPinned(true);
    scrollToBottom();
  }, [scrollToBottom]);

  const onScroll = useCallback(() => {
    const el = elRef.current;
    if (!el) return;
    if (Date.now() < programmaticUntilRef.current) return;
    const dist = el.scrollHeight - (el.scrollTop + el.clientHeight);
    const pinned = dist < BOTTOM_THRESHOLD_PX;
    userScrolledRef.current = !pinned;
    setIsPinned(pinned);
  }, []);

  const scrollRef = useCallback((el: HTMLDivElement | null) => {
    elRef.current = el;
  }, []);

  const contentRef = useCallback(
    (el: HTMLDivElement | null) => {
      contentElRef.current = el;
      if (!el || typeof ResizeObserver === "undefined") return;
      const observer = new ResizeObserver(() => {
        if (!userScrolledRef.current) scrollToBottom();
      });
      observer.observe(el);
      return () => observer.disconnect();
    },
    [scrollToBottom],
  );

  useEffect(() => {
    if (active && !userScrolledRef.current) scrollToBottom();
  }, [active, scrollToBottom]);

  return { scrollRef, contentRef, onScroll, isPinned, jumpToBottom };
}
