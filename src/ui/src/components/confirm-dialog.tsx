"use client";

import { createContext, useCallback, useContext, useRef, useState } from "react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";

export interface ConfirmOptions {
  title: string;
  description?: string;
  /** Label of the confirming button; defaults to 确认删除 for destructive. */
  confirmLabel?: string;
  cancelLabel?: string;
  /** Destructive styling for the confirm button (default true — this dialog
   *  exists for dangerous actions; use plain flows for benign ones). */
  destructive?: boolean;
}

type ConfirmFn = (options: ConfirmOptions) => Promise<boolean>;

const ConfirmContext = createContext<ConfirmFn | null>(null);

/** App-level provider that renders one styled confirmation dialog and exposes
 *  `useConfirm()` — an async replacement for window.confirm(). */
export function ConfirmDialogProvider({ children }: { children: React.ReactNode }) {
  const [options, setOptions] = useState<ConfirmOptions | null>(null);
  const resolveRef = useRef<((ok: boolean) => void) | null>(null);

  const confirm = useCallback<ConfirmFn>((next) => {
    return new Promise<boolean>((resolve) => {
      resolveRef.current?.(false);
      resolveRef.current = resolve;
      setOptions(next);
    });
  }, []);

  const settle = (ok: boolean) => {
    resolveRef.current?.(ok);
    resolveRef.current = null;
    setOptions(null);
  };

  return (
    <ConfirmContext.Provider value={confirm}>
      {children}
      <Dialog open={options !== null} onOpenChange={(open) => !open && settle(false)}>
        <DialogContent className="max-w-sm">
          {options && (
            <>
              <DialogHeader>
                <DialogTitle>{options.title}</DialogTitle>
                {options.description && (
                  <DialogDescription>{options.description}</DialogDescription>
                )}
              </DialogHeader>
              <DialogFooter>
                <Button variant="outline" size="sm" onClick={() => settle(false)}>
                  {options.cancelLabel ?? "取消"}
                </Button>
                <Button
                  variant={options.destructive === false ? "default" : "destructive"}
                  size="sm"
                  autoFocus
                  onClick={() => settle(true)}
                >
                  {options.confirmLabel ?? "确认删除"}
                </Button>
              </DialogFooter>
            </>
          )}
        </DialogContent>
      </Dialog>
    </ConfirmContext.Provider>
  );
}

/** Async confirmation: `if (!(await confirm({ title: … }))) return;` */
export function useConfirm(): ConfirmFn {
  const confirm = useContext(ConfirmContext);
  if (!confirm) throw new Error("useConfirm must be used within ConfirmDialogProvider");
  return confirm;
}
