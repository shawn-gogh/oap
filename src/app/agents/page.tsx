"use client";

import { useCallback, useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { Plus, RefreshCw } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { AgentAvatar } from "@/components/agent-avatar";
import {
  AgentRow,
  ApiError,
  SessionRow,
  listAgents,
  listSessions,
} from "@/lib/api";

interface RowState {
  agent: AgentRow;
  active: boolean;
}

function formatRelative(iso?: string | null): string {
  if (!iso) return "—";
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return iso;
  const diff = Date.now() - then;
  if (diff < 0) return "just now";
  const sec = Math.floor(diff / 1000);
  if (sec < 60) return `${sec}s`;
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min}m`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h`;
  const day = Math.floor(hr / 24);
  return `${day}d`;
}

export default function AgentsListPage() {
  const router = useRouter();
  const [rows, setRows] = useState<RowState[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [agents, sessions] = await Promise.all([
        listAgents(),
        listSessions(),
      ]);
      const activeAgentIds = new Set<string>(
        sessions
          .filter((s: SessionRow) => s.status === "ready")
          .map((s: SessionRow) => s.agent_id),
      );
      setRows(
        agents.map((a) => ({ agent: a, active: activeAgentIds.has(a.id) })),
      );
    } catch (e) {
      setError(e instanceof ApiError ? e.message : (e as Error).message);
      setRows([]);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <div className="mx-auto w-full max-w-4xl px-6 py-10">
      <header className="flex items-baseline justify-between">
        <div>
          <h1 className="text-[26px] font-semibold tracking-tight leading-none">
            Agents
          </h1>
          <p className="mt-2 text-sm text-muted-foreground">
            {rows.length} {rows.length === 1 ? "agent" : "agents"}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => void load()}
            disabled={loading}
            aria-label="Refresh"
            className="h-8 px-2 text-muted-foreground hover:text-foreground"
          >
            <RefreshCw className={loading ? "size-3.5 animate-spin" : "size-3.5"} />
          </Button>
          <Button size="sm" onClick={() => router.push("/agents/new")}>
            <Plus className="size-4" />
            New agent
          </Button>
        </div>
      </header>

      {error ? (
        <div className="mt-6 rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 font-mono text-xs text-destructive">
          {error}
        </div>
      ) : null}

      {rows.length === 0 && !loading ? (
        <div className="mt-10 rounded-lg border border-dashed bg-card/40 px-6 py-16 text-center">
          <p className="text-sm text-muted-foreground">No agents yet.</p>
          <Button
            size="sm"
            onClick={() => router.push("/agents/new")}
            className="mt-4"
          >
            <Plus className="size-4" />
            Create your first agent
          </Button>
        </div>
      ) : (
        <ul className="mt-8 overflow-hidden rounded-lg border bg-card">
          {rows.map(({ agent, active }, i) => (
            <li
              key={agent.id}
              onClick={() => router.push(`/agents/${agent.id}`)}
              className={
                "flex cursor-pointer items-center gap-4 px-4 py-4 transition-colors hover:bg-muted/50 " +
                (i > 0 ? "border-t" : "")
              }
            >
              <div className="relative shrink-0">
                <AgentAvatar
                  name={agent.name ?? agent.id}
                  pfpUrl={agent.pfp_url}
                  size={48}
                />
                {active ? (
                  <span
                    aria-label="active"
                    title="active session"
                    className="absolute -right-0.5 -bottom-0.5 size-3 rounded-full bg-emerald-500 ring-2 ring-card"
                  />
                ) : null}
              </div>
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span className="truncate text-[15px] font-medium">
                    {agent.name?.trim() || (
                      <span className="text-muted-foreground">Untitled agent</span>
                    )}
                  </span>
                  <Badge variant="secondary" className="shrink-0 font-mono text-[10px]">
                    {agent.model}
                  </Badge>
                </div>
                <div className="mt-0.5 truncate text-[13px] text-muted-foreground">
                  {agent.prompt?.trim() || (
                    <span className="font-mono text-[11px] text-muted-foreground/70">
                      {agent.id}
                    </span>
                  )}
                </div>
              </div>
              <span className="hidden shrink-0 font-mono text-[12px] text-muted-foreground sm:inline">
                {agent.branch}
              </span>
              <span className="shrink-0 tabular-nums text-[12px] text-muted-foreground">
                {formatRelative(agent.created_at)}
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
