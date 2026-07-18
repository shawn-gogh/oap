"use client";

import { useEffect, useState } from "react";

export function useCurrentTime(intervalMs = 60_000): number | null {
  const [currentTime, setCurrentTime] = useState<number | null>(null);

  useEffect(() => {
    const update = () => setCurrentTime(Date.now());
    const initial = window.setTimeout(update, 0);
    const interval = window.setInterval(update, intervalMs);
    return () => {
      window.clearTimeout(initial);
      window.clearInterval(interval);
    };
  }, [intervalMs]);

  return currentTime;
}
