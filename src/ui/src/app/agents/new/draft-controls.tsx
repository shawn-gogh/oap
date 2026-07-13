"use client";

import { useState } from "react";
import { ExternalLink, KeyRound, Plug, Plus, Wrench, X } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { EditorChip } from "@/components/editor-chip";
import { BrandIcon } from "@/components/brand-icons";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { ModelSelect } from "@/components/model-select";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
} from "@/components/ui/select";
import { ScheduleEditor } from "@/components/schedule-editor";
import { Textarea } from "@/components/ui/textarea";
import type { AgentDraft } from "@/lib/agent-builder";
import type { Integration } from "@/lib/integrations";
import { modelOptions } from "@/lib/model-options";
import { runtimeBrandIconId } from "@/lib/runtime-branding";
import type { Agent, AgentRuntime, AgentRuntimeTool, Rule, RuntimeHarness, Skill } from "@/lib/types";
import { cn } from "@/lib/utils";

export function AgentDraftControls({
  agents,
  harnesses,
  draft,
  mcpError,
  mcpIntegrations,
  mcpLoading,
  models,
  modelsError,
  modelsLoading,
  rules,
  skills,
  runtimes,
  onChange,
}: {
  agents: Agent[];
  harnesses: RuntimeHarness[];
  draft: AgentDraft;
  mcpError: string | null;
  mcpIntegrations: Integration[];
  mcpLoading: boolean;
  models: string[];
  modelsError: string | null;
  modelsLoading: boolean;
  rules: Rule[];
  skills: Skill[];
  runtimes: AgentRuntime[];
  onChange: (next: AgentDraft) => void;
}) {
  const update = (patch: Partial<AgentDraft>) => onChange({ ...draft, ...patch });
  const availableModels = modelOptions(models, draft.model);
  const runtime = runtimes.find((entry) => entry.id === draft.runtime);
  const selectedHarness = harnesses.find((entry) => entry.alias === draft.runtime);
  const runtimeTools = selectedHarness?.tools ?? runtime?.tools ?? [];
  const toolOptions = runtimeTools.length > 0
    ? runtimeTools.map((tool) => tool.id).filter(Boolean)
    : draft.tools.map((tool) => tool.type).filter(Boolean);
  const toolRisk = new Map(
    runtimeTools.filter((tool) => tool.risk).map((tool) => [tool.id, tool.risk as string]),
  );
  const selectedTools = new Set(draft.tools.map((tool) => tool.type).filter(Boolean));
  const selectedSubAgents = new Set(draft.sub_agents.map((agent) => agent.agent_id));
  const [vaultKeyInput, setVaultKeyInput] = useState("");
  const [vaultKeyError, setVaultKeyError] = useState<string | null>(null);
  const setTool = (toolId: string, enabled: boolean) => {
    const next = new Set(selectedTools);
    if (enabled) next.add(toolId);
    else next.delete(toolId);
    update({ tools: Array.from(next).map((type) => ({ type })) });
  };
  const toggleRule = (ruleId: string, enabled: boolean) => {
    update({
      rule_ids: enabled
        ? Array.from(new Set([...draft.rule_ids, ruleId]))
        : draft.rule_ids.filter((id) => id !== ruleId),
    });
  };
  const toggleSkill = (skillId: string, enabled: boolean) => {
    update({
      skill_ids: enabled
        ? Array.from(new Set([...draft.skill_ids, skillId]))
        : draft.skill_ids.filter((id) => id !== skillId),
    });
  };
  const toggleMcpIntegration = (integrationId: string, enabled: boolean) => {
    update({
      mcp_server_ids: enabled
        ? Array.from(new Set([...draft.mcp_server_ids, integrationId]))
        : draft.mcp_server_ids.filter((id) => id !== integrationId),
    });
  };
  const addVaultKey = () => {
    const key = vaultKeyInput.trim();
    if (!key) return;
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(key)) {
      setVaultKeyError("密钥名只能包含字母、数字和下划线，且不能以数字开头，例如 BROWSER_USE_API_KEY。");
      return;
    }
    setVaultKeyError(null);
    update({ vault_keys: Array.from(new Set([...draft.vault_keys, key])) });
    setVaultKeyInput("");
  };

  return (
    <div className="min-h-0 flex-1 overflow-y-auto bg-editor-surface px-5 py-4 text-editor-foreground">
      <div className="mx-auto grid max-w-3xl gap-4">
        <div className="grid gap-1.5">
          <Label htmlFor="draft-name" className="text-editor-muted">
            Name
          </Label>
          <Input
            id="draft-name"
            value={draft.name}
            onChange={(event) => update({ name: event.target.value })}
            placeholder="security-reviewer"
            className="border-white/10 bg-editor-surface-raised text-editor-foreground placeholder:text-editor-faint"
          />
        </div>

        <div className="grid gap-1.5">
          <Label htmlFor="draft-description" className="text-editor-muted">
            Description
          </Label>
          <Input
            id="draft-description"
            value={draft.description}
            onChange={(event) => update({ description: event.target.value })}
            placeholder="这个智能体做什么"
            className="border-white/10 bg-editor-surface-raised text-editor-foreground placeholder:text-editor-faint"
          />
        </div>

        <div className="grid gap-1.5">
          <Label className="text-editor-muted">Model</Label>
          <div className="[&_button]:border-white/10 [&_button]:bg-editor-surface-raised [&_button]:text-editor-foreground [&_svg]:text-editor-faint">
            <ModelSelect
              value={draft.model}
              models={availableModels}
              onValueChange={(model) => update({ model })}
            />
          </div>
          {modelsLoading && (
            <p className="text-xs text-editor-faint">正在加载可用模型...</p>
          )}
          {modelsError && (
            <p className="text-xs text-red-300">{modelsError}</p>
          )}
        </div>

        {harnesses.length === 0 && (
          <div className="grid gap-1.5">
            <Label className="text-editor-muted">Runtime</Label>
            <div className="rounded-md border border-white/10 bg-white/5 px-3 py-3 text-xs text-editor-faint">
              <p>没有已连接的运行时 harness，当前使用默认运行时 {runtimeLabel(draft.runtime)}。</p>
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={() => {
                  window.location.href = "/runtimes/";
                }}
                className="mt-2 border-white/10 bg-white/5 text-editor-foreground hover:bg-white/10 hover:text-white"
              >
                <ExternalLink className="size-3.5" />
                去连接运行时
              </Button>
            </div>
          </div>
        )}
        {harnesses.length >= 1 && (
          <div className="grid gap-1.5">
            <Label className="text-editor-muted">Runtime</Label>
            <Select
              value={draft.runtime || "claude_managed_agents"}
              onValueChange={(v) => {
                const runtimeAlias = v ?? "claude_managed_agents";
                update({
                  runtime: runtimeAlias,
                  model: "",
                  tools: defaultToolsForHarnessRuntime(runtimeAlias, harnesses, runtimes),
                });
              }}
            >
              <SelectTrigger className="h-11 w-full max-w-sm overflow-hidden border-white/10 bg-editor-surface-raised px-3 text-editor-foreground">
                <RuntimeSelectOption
                  alias={draft.runtime || "claude_managed_agents"}
                  displayName={selectedHarness?.display_name ?? runtime?.name ?? runtimeLabel(draft.runtime)}
                  apiSpec={selectedHarness?.api_spec ?? runtimeApiSpec(draft.runtime)}
                  isDefault={selectedHarness?.is_default}
                  compact
                />
              </SelectTrigger>
              <SelectContent className="w-[360px] border-white/10 bg-editor-surface-raised text-editor-foreground">
                {harnesses.map((h) => (
                  <SelectItem
                    key={h.alias}
                    value={h.alias}
                    className="py-3 focus:bg-white/10 focus:text-editor-foreground data-highlighted:bg-white/10 data-highlighted:text-editor-foreground [&_span]:!text-editor-foreground [&_.runtime-option-muted]:!text-editor-muted [&_svg]:!text-editor-foreground"
                  >
                    <RuntimeSelectOption
                      alias={h.alias}
                      displayName={h.display_name}
                      apiSpec={h.api_spec}
                      isDefault={h.is_default}
                    />
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <p className="text-xs text-editor-faint">
              切换运行时会把已选工具重置为该运行时的默认工具集，并重新加载可用模型。
            </p>
          </div>
        )}

        <div className="grid gap-1.5">
          <Label htmlFor="draft-system" className="text-editor-muted">
            System prompt
          </Label>
          <Textarea
            id="draft-system"
            value={draft.system}
            onChange={(event) => update({ system: event.target.value })}
            className="min-h-[280px] resize-y border-white/10 bg-editor-surface-raised font-mono text-xs text-editor-foreground placeholder:text-editor-faint"
            placeholder="You are a meticulous security reviewer..."
          />
        </div>

        <div className="[&_button]:border-white/10 [&_button]:bg-editor-surface-raised [&_button]:text-editor-foreground [&_input]:border-white/10 [&_input]:bg-editor-surface-raised [&_input]:text-editor-foreground [&_label]:text-editor-muted [&_section]:border-white/10 [&_section]:bg-black/10 [&_svg]:text-editor-faint">
          <ScheduleEditor
            cron={draft.cron}
            timezone={draft.timezone}
            onChange={(schedule) => update(schedule)}
          />
        </div>

        <div className="grid gap-2 rounded-md border border-white/10 bg-black/10 p-3">
          <div className="flex items-start justify-between gap-3">
            <div className="grid gap-1">
              <Label className="text-sm font-medium">Vault Credentials</Label>
              <p className="max-w-xl text-xs leading-5 text-editor-faint">
                先登记密钥名称，创建后在智能体详情页填写密钥值。
              </p>
            </div>
            <span className="shrink-0 font-mono text-xs text-editor-faint">
              {draft.vault_keys.length} 已挂载
            </span>
          </div>
          <div className="flex gap-2">
            <Input
              value={vaultKeyInput}
              onChange={(event) => {
                setVaultKeyInput(event.target.value);
                setVaultKeyError(null);
              }}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  addVaultKey();
                }
              }}
              placeholder="BROWSER_USE_API_KEY"
              className="border-white/10 bg-white/5 font-mono text-xs"
              aria-label="Vault credential name"
            />
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={addVaultKey}
              disabled={!vaultKeyInput.trim()}
              className="border-white/10 bg-white/5 hover:bg-white/10"
            >
              <Plus className="size-3.5" />
              添加密钥
            </Button>
          </div>
          {vaultKeyError && <p className="text-xs text-red-300">{vaultKeyError}</p>}
          {draft.vault_keys.length === 0 ? (
            <p className="text-xs text-editor-faint">尚未挂载保险库凭证。</p>
          ) : (
            <div className="flex flex-wrap gap-1.5">
              {draft.vault_keys.map((key) => (
                <span
                  key={key}
                  className="inline-flex max-w-full items-center gap-1 rounded-md border border-white/10 bg-white/5 px-2 py-1"
                >
                  <KeyRound className="size-3 shrink-0 text-editor-faint" />
                  <span className="truncate font-mono text-xs">{key}</span>
                  <button
                    type="button"
                    className="rounded text-editor-faint hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
                    onClick={() =>
                      update({ vault_keys: draft.vault_keys.filter((value) => value !== key) })
                    }
                    aria-label={`Remove ${key}`}
                  >
                    <X className="size-3" />
                  </button>
                </span>
              ))}
            </div>
          )}
        </div>

        <div className="grid gap-2 rounded-md border border-white/10 bg-black/10 p-3 text-editor-foreground">
          <div className="flex items-center justify-between gap-3">
            <Label className="text-sm font-medium">Tools</Label>
            <span className="font-mono text-xs text-editor-faint">
              {draft.tools.length} 已选
            </span>
          </div>
          <div className="grid max-h-[284px] gap-2 overflow-y-auto pr-1 sm:grid-cols-2">
            {toolOptions.map((toolId) => {
              const risk = toolRisk.get(toolId);
              return (
                <label
                  key={toolId}
                  className="flex min-w-0 cursor-pointer items-start gap-2 rounded-md border border-white/10 bg-white/5 px-2.5 py-2 text-xs hover:bg-white/10"
                >
                  <input
                    type="checkbox"
                    checked={selectedTools.has(toolId)}
                    onChange={(event) => setTool(toolId, event.target.checked)}
                    className="mt-0.5 size-3.5 shrink-0"
                  />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate font-mono">{toolId}</span>
                    {risk && (
                      <span className="mt-0.5 block text-[11px] leading-snug text-amber-500/90">
                        {risk}
                      </span>
                    )}
                  </span>
                </label>
              );
            })}
          </div>
        </div>

        <div className="grid gap-2 rounded-md border border-white/10 bg-black/10 p-3 text-editor-foreground">
          <div className="flex items-center justify-between gap-3">
            <Label className="text-sm font-medium">Skills</Label>
            <span className="font-mono text-xs text-editor-faint">
              {draft.skill_ids.length} 已挂载
            </span>
          </div>
          {skills.length === 0 ? (
            <p className="text-xs text-editor-faint">暂无可用技能。</p>
          ) : (
            <div className="grid max-h-[284px] gap-2 overflow-y-auto pr-1">
              {skills.map((skill) => {
                const enabled = draft.skill_ids.includes(skill.id);
                return (
                  <label
                    key={skill.id}
                    className="flex min-w-0 cursor-pointer items-start gap-2.5 rounded-md border border-white/10 bg-white/5 px-2.5 py-2 text-xs hover:bg-white/10"
                  >
                    <input
                      type="checkbox"
                      checked={enabled}
                      onChange={(event) => toggleSkill(skill.id, event.target.checked)}
                      className="mt-0.5 size-3.5 shrink-0"
                    />
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span className="font-medium">{skill.name}</span>
                        <span className="truncate font-mono text-editor-faint">{skill.id}</span>
                      </div>
                      <div className="mt-0.5 line-clamp-2 text-editor-faint">
                        {skill.description || "暂无描述。"}
                      </div>
                    </div>
                  </label>
                );
              })}
            </div>
          )}
        </div>

        <div className="grid gap-2 rounded-md border border-white/10 bg-black/10 p-3 text-editor-foreground">
          <div className="flex items-center justify-between gap-3">
            <div className="grid gap-1">
              <Label className="text-sm font-medium">MCP integrations</Label>
              <p className="max-w-xl text-xs leading-5 text-editor-faint">
                从注册表挂载托管 MCP 服务器。创建智能体时会根据这些 ID 重建工具集。
              </p>
            </div>
            <span className="font-mono text-xs text-editor-faint">
              {draft.mcp_server_ids.length} 已挂载
            </span>
          </div>
          {mcpError && (
            <div className="rounded-md border border-destructive/30 bg-destructive/10 px-2.5 py-2 text-xs text-destructive">
              {mcpError}
            </div>
          )}
          {mcpLoading ? (
            <div className="grid gap-2">
              {[0, 1, 2].map((item) => (
                <div
                  key={item}
                  className="rounded-md border border-white/10 bg-white/5 px-2.5 py-3"
                >
                  <div className="h-3 w-1/3 animate-pulse rounded bg-muted motion-reduce:animate-none" />
                  <div className="mt-2 h-3 w-2/3 animate-pulse rounded bg-muted motion-reduce:animate-none" />
                </div>
              ))}
            </div>
          ) : mcpIntegrations.length === 0 ? (
            <div className="rounded-md border border-white/10 bg-white/5 px-3 py-4 text-center">
              <Plug className="mx-auto size-6 text-editor-faint" />
              <p className="mt-2 text-xs font-medium">暂无可用的 MCP 服务器</p>
              <p className="mt-1 text-xs text-editor-faint">
                先到 MCP 注册表添加服务器，再回到这里挂载。
              </p>
            </div>
          ) : (
            <div className="grid max-h-[360px] gap-2 overflow-y-auto pr-1">
              {mcpIntegrations.map((integration) => {
                const enabled = draft.mcp_server_ids.includes(integration.id);
                const availableTools = integration.tools.filter(Boolean);
                const previewTools = availableTools.slice(0, 8);
                const remainingTools = Math.max(availableTools.length - previewTools.length, 0);
                const canAttach = integration.mcpUrl.trim().length > 0;
                return (
                  <label
                    key={integration.id}
                    className={cn(
                      "flex min-w-0 cursor-pointer items-start gap-2.5 rounded-md border border-white/10 bg-white/5 px-2.5 py-2.5 text-xs hover:bg-white/10",
                      enabled && "border-white/30 bg-white/10",
                      !canAttach && "cursor-not-allowed opacity-70",
                    )}
                  >
                    <input
                      type="checkbox"
                      checked={enabled}
                      disabled={!canAttach}
                      onChange={(event) => toggleMcpIntegration(integration.id, event.target.checked)}
                      className="mt-0.5 size-3.5 shrink-0"
                    />
                    <div className="min-w-0 flex-1">
                      <div className="flex flex-wrap items-center gap-2">
                        <span className="font-medium">{integration.name}</span>
                        <span className="truncate font-mono text-editor-faint">{integration.id}</span>
                        <Badge variant="outline" className="h-5 rounded-md border-white/10 bg-white/5 text-[11px] text-editor-muted">
                          {integration.source === "registry" ? "注册表" : "目录"}
                        </Badge>
                        {integration.connected ? (
                          <Badge variant="secondary" className="h-5 rounded-md text-[11px]">
                            <KeyRound className="size-3" />
                            已连接
                          </Badge>
                        ) : (
                          <Badge variant="outline" className="h-5 rounded-md border-white/10 bg-white/5 text-[11px] text-editor-muted">
                            <KeyRound className="size-3" />
                            待配置凭证
                          </Badge>
                        )}
                      </div>
                      <div className="mt-1 line-clamp-2 text-editor-faint">
                        {integration.description}
                      </div>
                      <div className="mt-1 flex flex-wrap items-center gap-2 text-[11px] text-editor-faint">
                        <span className="inline-flex items-center gap-1">
                          <KeyRound className="size-3" />
                          {integration.envKey}
                        </span>
                        <span className="inline-flex items-center gap-1">
                          <Wrench className="size-3" />
                          {availableTools.length > 0
                            ? `${availableTools.length} 个可用工具`
                            : "尚未发现工具"}
                        </span>
                      </div>
                      {(enabled || availableTools.length > 0) && (
                        <div className="mt-2 flex flex-wrap gap-1">
                          {previewTools.map((tool) => (
                            <EditorChip key={tool}>{tool}</EditorChip>
                          ))}
                          {remainingTools > 0 && (
                            <EditorChip>+{remainingTools} more</EditorChip>
                          )}
                        </div>
                      )}
                      {!canAttach && (
                        <p className="mt-2 text-xs text-destructive">
                          该服务器缺少 URL，暂时无法挂载到托管智能体。
                        </p>
                      )}
                    </div>
                  </label>
                );
              })}
            </div>
          )}
          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={() => {
              window.location.href = "/mcp-servers/";
            }}
            className="justify-self-start border-white/10 bg-white/5 text-editor-foreground hover:bg-white/10 hover:text-white"
          >
            <ExternalLink className="size-3.5" />
            管理 MCP 服务器
          </Button>
        </div>

        <div className="grid gap-2 rounded-md border border-white/10 bg-black/10 p-3 text-editor-foreground">
          <div className="flex items-start justify-between gap-3">
            <div className="grid gap-1">
              <Label className="text-sm font-medium">Rules</Label>
              <p className="max-w-xl text-xs leading-5 text-editor-faint">
                规则是持久的 prompt 级指令。挂载后其 Markdown 内容会在模型运行前注入智能体上下文。
              </p>
            </div>
            <span className="shrink-0 font-mono text-xs text-editor-faint">
              {draft.rule_ids.length} 已挂载
            </span>
          </div>
          {rules.length === 0 ? (
            <p className="text-xs text-editor-faint">暂无可用规则。</p>
          ) : (
            <div className="grid max-h-[284px] gap-2 overflow-y-auto pr-1">
              {rules.map((rule) => {
                const enabled = draft.rule_ids.includes(rule.id);
                return (
                  <label
                    key={rule.id}
                    className="flex min-w-0 cursor-pointer items-start gap-2.5 rounded-md border border-white/10 bg-white/5 px-2.5 py-2 text-xs hover:bg-white/10"
                  >
                    <input
                      type="checkbox"
                      checked={enabled}
                      onChange={(event) => toggleRule(rule.id, event.target.checked)}
                      className="mt-0.5 size-3.5 shrink-0"
                    />
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span className="font-medium">{rule.name}</span>
                        <span className="truncate font-mono text-editor-faint">{rule.id}</span>
                      </div>
                      <div className="mt-0.5 line-clamp-2 text-editor-faint">
                        {rule.description || "暂无描述。"}
                      </div>
                    </div>
                  </label>
                );
              })}
            </div>
          )}
        </div>

        <div className="grid gap-2 rounded-md border border-white/10 bg-black/10 p-3 text-editor-foreground">
          <div className="flex items-center justify-between gap-3">
            <Label className="text-sm font-medium">Sub-agents</Label>
            <span className="font-mono text-xs text-editor-faint">
              {draft.sub_agents.length} 已挂载
            </span>
          </div>
          {agents.length === 0 ? (
            <div className="rounded-md border border-white/10 bg-white/5 px-2.5 py-2 text-xs text-editor-faint">
              请先创建辅助智能体，再回到这里挂载。
            </div>
          ) : (
            <div className="grid max-h-[284px] gap-2 overflow-y-auto pr-1">
              {agents.map((agent) => {
                const enabled = selectedSubAgents.has(agent.id);
                const toggle = (on: boolean) => {
                  const next = on
                    ? [...draft.sub_agents, { agent_id: agent.id }]
                    : draft.sub_agents.filter((entry) => entry.agent_id !== agent.id);
                  update({ sub_agents: next });
                };
                return (
                  <label
                    key={agent.id}
                    className="flex min-w-0 cursor-pointer items-start gap-2.5 rounded-md border border-white/10 bg-white/5 px-2.5 py-2 text-xs hover:bg-white/10"
                  >
                    <input
                      type="checkbox"
                      checked={enabled}
                      onChange={(event) => toggle(event.target.checked)}
                      className="mt-0.5 size-3.5 shrink-0"
                    />
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span className="truncate font-medium">{agent.name}</span>
                        <span className="truncate font-mono text-editor-faint">{agent.id}</span>
                      </div>
                      <div className="mt-0.5 line-clamp-2 text-editor-faint">
                        {agent.description || agent.model || "已保存的 LAP 智能体"}
                      </div>
                    </div>
                  </label>
                );
              })}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function RuntimeSelectOption({
  alias,
  displayName,
  apiSpec,
  isDefault,
  compact = false,
}: {
  alias: string;
  displayName: string;
  apiSpec: string;
  isDefault?: boolean;
  compact?: boolean;
}) {
  return (
    <span className={cn("flex min-w-0 max-w-full items-center", compact ? "gap-2" : "gap-3")}>
      <span
        className={cn(
          "flex shrink-0 items-center justify-center rounded-lg border border-white/10 bg-white/5",
          compact ? "size-6" : "size-8",
        )}
      >
        <BrandIcon id={runtimeBrandIconId(alias, apiSpec)} className={compact ? "size-3.5" : "size-4"} />
      </span>
      <span className="min-w-0">
        <span className="flex min-w-0 items-center gap-2">
          <span className="truncate text-sm font-medium !text-editor-foreground">{displayName}</span>
          {isDefault && !compact && (
            <span className="runtime-option-muted rounded-md border border-white/10 bg-white/5 px-1.5 py-0.5 text-[11px] !text-editor-muted">
              Default
            </span>
          )}
        </span>
        <span className="runtime-option-muted mt-0.5 block truncate font-mono text-[11px] !text-editor-muted">
          {compact ? runtimeSubtitle(apiSpec || alias) : `${runtimeLabel(apiSpec || alias)} · ${alias}`}
        </span>
      </span>
    </span>
  );
}

function defaultToolsForHarnessRuntime(
  runtimeAlias: string,
  harnesses: RuntimeHarness[],
  runtimes: AgentRuntime[],
): AgentDraft["tools"] {
  const tools =
    harnesses.find((entry) => entry.alias === runtimeAlias)?.tools ??
    runtimes.find((entry) => entry.id === runtimeAlias)?.tools ??
    [];
  return tools
    .filter((tool: AgentRuntimeTool) => tool.enabled_by_default)
    .map((tool) => ({ type: tool.id }));
}

function runtimeApiSpec(value: string): string {
  if (value === "claude_managed_agents") return "claude_managed_agents";
  if (value === "cursor") return "cursor";
  if (value === "gemini_antigravity") return "gemini_antigravity";
  if (value === "opencode") return "opencode";
  return value;
}

function runtimeLabel(value: string): string {
  if (value === "claude_managed_agents") return "Claude Managed Agents";
  if (value === "cursor") return "Cursor";
  if (value === "gemini_antigravity") return "Gemini Antigravity";
  if (value === "opencode") return "OpenCode";
  return value || "Runtime";
}

function runtimeSubtitle(value: string): string {
  if (value === "claude_managed_agents") return "Anthropic sessions and tools";
  if (value === "cursor") return "Background repo agents";
  if (value === "gemini_antigravity") return "Google managed sandbox";
  if (value === "opencode") return "OpenCode server";
  return "Custom runtime";
}
