"use client";

import { Badge } from "@/components/ui/badge";
import { EditorChip } from "@/components/editor-chip";
import type { AgentDraft } from "@/lib/agent-builder";
import type { Integration } from "@/lib/integrations";
import { scheduleLabel } from "@/lib/schedule";

export function ConfigPreview({
  draft,
  mcpIntegrations,
}: {
  draft: AgentDraft;
  mcpIntegrations: Integration[];
}) {
  const selectedMcpIntegrations = draft.mcp_server_ids.map((id) => {
    const integration = mcpIntegrations.find((item) => item.id === id);
    return integration ?? {
      id,
      name: id,
      description: "Unknown MCP server.",
      category: "Other",
      envKey: "Unknown",
      mcpUrl: "",
      tools: [],
      source: "catalog" as const,
      connected: false,
      status: null,
    };
  });

  return (
    <div className="min-h-0 flex-1 overflow-y-auto rounded-b-lg bg-editor-surface px-5 py-4">
      <div className="grid gap-5">
        <div>
          <div className="text-xs uppercase text-editor-faint">Name</div>
          <div className="mt-1 text-xl font-semibold text-editor-foreground">{draft.name}</div>
          <p className="mt-2 max-w-2xl text-sm leading-6 text-editor-muted">{draft.description}</p>
        </div>

        <div className="grid gap-3 sm:grid-cols-2">
          <PreviewItem label="Model" value={draft.model} />
          <PreviewItem label="Runtime" value={draft.runtime} />
          <PreviewItem label="Schedule" value={scheduleLabel(draft.cron, draft.timezone)} />
          <PreviewItem label="Tools" value={draft.tools.map((tool) => tool.type).filter(Boolean).join(", ")} />
        </div>

        <div>
          <div className="text-xs uppercase text-editor-faint">System prompt</div>
          <pre className="mt-2 max-h-80 overflow-y-auto whitespace-pre-wrap rounded-lg border border-white/10 bg-black/15 p-3 font-mono text-xs leading-6 text-editor-accent">
            {draft.system || "未设置 system prompt。"}
          </pre>
        </div>

        <div className="grid gap-3 sm:grid-cols-2">
          <TokenList label="Vault keys" values={draft.vault_keys} />
          <TokenList label="Skill IDs" values={draft.skill_ids} />
          <TokenList label="Rule IDs" values={draft.rule_ids} />
          <TokenList label="Sub-agents" values={draft.sub_agents.map((agent) => agent.agent_id)} />
        </div>

        <div className="rounded-lg border border-white/10 bg-black/10 p-3">
          <div className="text-xs uppercase text-editor-faint">MCP integrations</div>
          {selectedMcpIntegrations.length === 0 ? (
            <div className="mt-2 text-xs text-editor-muted">None</div>
          ) : (
            <div className="mt-3 grid gap-2">
              {selectedMcpIntegrations.map((integration) => {
                const toolCount = integration.tools.filter(Boolean).length;
                return (
                  <div
                    key={integration.id}
                    className="rounded-md border border-white/10 bg-white/5 px-2.5 py-2"
                  >
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="text-xs font-medium text-editor-foreground">{integration.name}</span>
                      <span className="font-mono text-[11px] text-editor-faint">{integration.id}</span>
                      <Badge variant="outline" className="h-5 rounded-md border-white/10 bg-white/5 text-[11px] text-editor-muted">
                        {toolCount > 0 ? `${toolCount} tools` : "Toolset attached"}
                      </Badge>
                    </div>
                    <p className="mt-1 line-clamp-2 text-xs text-editor-muted">{integration.description}</p>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function PreviewItem({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-white/10 bg-black/10 p-3">
      <div className="text-xs uppercase text-editor-faint">{label}</div>
      <div className="mt-1 break-words font-mono text-xs text-editor-foreground">{value || "未设置"}</div>
    </div>
  );
}

function TokenList({ label, values }: { label: string; values: string[] }) {
  return (
    <div className="rounded-lg border border-white/10 bg-black/10 p-3">
      <div className="text-xs uppercase text-editor-faint">{label}</div>
      {values.length === 0 ? (
        <div className="mt-2 text-xs text-editor-muted">None</div>
      ) : (
        <div className="mt-2 flex flex-wrap gap-1.5">
          {values.map((value) => (
            <EditorChip key={value} className="rounded-md text-editor-foreground">
              {value}
            </EditorChip>
          ))}
        </div>
      )}
    </div>
  );
}

