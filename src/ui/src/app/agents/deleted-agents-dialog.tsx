"use client";

import { useEffect, useState } from "react";
import { toast } from "sonner";
import { RotateCcw, Trash2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { EmptyState } from "@/components/empty-state";
import { apiErrorMessage, listDeletedAgents, restoreAgent } from "@/lib/api";
import type { Agent } from "@/lib/types";

const RETENTION_DAYS = 7;

function deletedAt(agent: Agent): number | null {
  const config = agent.config;
  if (!config || typeof config !== "object" || Array.isArray(config)) return null;
  const value = (config as { deleted_at?: unknown }).deleted_at;
  return typeof value === "number" ? value : null;
}

function remainingLabel(agent: Agent): string {
  const at = deletedAt(agent);
  if (at == null) return "";
  const purgeAt = at + RETENTION_DAYS * 24 * 60 * 60 * 1000;
  const daysLeft = Math.max(0, Math.ceil((purgeAt - Date.now()) / (24 * 60 * 60 * 1000)));
  return daysLeft <= 0 ? "即将永久删除" : `还剩 ${daysLeft} 天后永久删除`;
}

interface DeletedAgentsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onRestored: (agent: Agent) => void;
}

export function DeletedAgentsDialog({ open, onOpenChange, onRestored }: DeletedAgentsDialogProps) {
  const [agents, setAgents] = useState<Agent[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [restoringId, setRestoringId] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setAgents(null);
    setError(null);
    listDeletedAgents()
      .then(setAgents)
      .catch((err) => setError(apiErrorMessage(err, "加载已删除智能体失败")));
  }, [open]);

  const restore = async (agent: Agent) => {
    setRestoringId(agent.id);
    try {
      await restoreAgent(agent.id);
      setAgents((current) => current?.filter((item) => item.id !== agent.id) ?? null);
      onRestored({ ...agent, status: "draft" });
      toast.success(`已复原智能体「${agent.name}」`);
    } catch (err) {
      toast.error(apiErrorMessage(err, "复原失败"));
    } finally {
      setRestoringId(null);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="w-[92vw] sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>已删除的智能体</DialogTitle>
        </DialogHeader>
        <p className="text-xs text-muted-foreground">
          删除后 {RETENTION_DAYS} 天内可以复原；超过这个期限会被后台任务永久清除，包括工作区文件和评估历史。
        </p>
        <div className="max-h-[60vh] overflow-y-auto">
          {error ? (
            <p className="p-3 text-sm text-destructive">{error}</p>
          ) : !agents ? (
            <div className="grid gap-2 p-1">
              {[0, 1].map((item) => (
                <div key={item} className="h-14 animate-pulse rounded-lg bg-muted/40" />
              ))}
            </div>
          ) : agents.length === 0 ? (
            <EmptyState
              icon={Trash2}
              title="没有已删除的智能体"
              hint="最近删除的智能体会在这里保留一段时间。"
            />
          ) : (
            <ul className="grid gap-2">
              {agents.map((agent) => (
                <li
                  key={agent.id}
                  className="flex items-center justify-between gap-3 rounded-lg border border-border px-3 py-2.5"
                >
                  <div className="min-w-0">
                    <div className="truncate text-sm font-medium">{agent.name}</div>
                    <div className="mt-0.5 truncate text-xs text-muted-foreground">
                      {remainingLabel(agent)}
                    </div>
                  </div>
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={restoringId === agent.id}
                    onClick={() => void restore(agent)}
                    className="shrink-0 gap-1.5"
                  >
                    <RotateCcw className="size-3.5" />
                    {restoringId === agent.id ? "复原中…" : "复原"}
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
