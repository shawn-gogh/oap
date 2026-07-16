"use client";

import { useEffect, useState } from "react";

const KEY = "sidebar-collapsed";

export function useSidebarCollapsed() {
  const [collapsed, setCollapsedState] = useState<boolean>(() => {
    if (typeof window === "undefined") return false;
    return localStorage.getItem(KEY) === "1";
  });

  // Reflect changes made in other tabs so the rail stays consistent.
  useEffect(() => {
    const onStorage = (event: StorageEvent) => {
      if (event.key === KEY) setCollapsedState(event.newValue === "1");
    };
    window.addEventListener("storage", onStorage);
    return () => window.removeEventListener("storage", onStorage);
  }, []);

  const setCollapsed = (value: boolean) => {
    localStorage.setItem(KEY, value ? "1" : "0");
    setCollapsedState(value);
  };

  return [collapsed, setCollapsed] as const;
}
