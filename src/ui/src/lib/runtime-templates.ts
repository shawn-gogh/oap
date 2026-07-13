import type { BuiltinRuntimeId } from "@/lib/types";

export interface RuntimeTemplate {
  id: string;
  name: string;
  description: string;
  repoPath: string;
  repoUrl?: string;
  runtimeAlias: string;
  apiSpec: BuiltinRuntimeId;
}

// OAP bundles its runtime template catalog rather than fetching a manifest
// from the network at runtime (self-contained deployment requirement) —
// this list is the single source of truth.
export const RUNTIME_TEMPLATES: RuntimeTemplate[] = [
  {
    id: "deepagents",
    name: "Deep Agents",
    description: "LangChain Deep Agents, exposed through OAP's self-hosted open-harness protocol bridge.",
    repoPath: "templates/deepagents",
    repoUrl:
      "https://github.com/LiteLLM-Labs/litellm-agent-platform/tree/main/templates/deepagents",
    runtimeAlias: "deepagents",
    apiSpec: "claude_managed_agents",
  },
  {
    id: "pydantic-deepagents",
    name: "Pydantic Deep Agents",
    description: "Pydantic Deep Agents, exposed through OAP's self-hosted open-harness protocol bridge.",
    repoPath: "templates/pydantic-deepagents",
    repoUrl:
      "https://github.com/LiteLLM-Labs/litellm-agent-platform/tree/main/templates/pydantic-deepagents",
    runtimeAlias: "pydantic-deepagents",
    apiSpec: "claude_managed_agents",
  },
  {
    id: "hermes",
    name: "Hermes Agent",
    description: "Nous Research Hermes Agent, exposed through OAP's self-hosted open-harness protocol bridge.",
    repoPath: "templates/hermes",
    repoUrl: "https://github.com/LiteLLM-Labs/litellm-agent-platform/tree/main/templates/hermes",
    runtimeAlias: "hermes",
    apiSpec: "claude_managed_agents",
  },
  {
    id: "opencode",
    name: "OpenCode Bridge",
    description: "OpenCode agent server that OAP can register as a self-hosted open-harness runtime.",
    repoPath: "templates/opencode",
    repoUrl:
      "https://github.com/LiteLLM-Labs/litellm-agent-platform/tree/main/templates/opencode",
    runtimeAlias: "opencode-anthropic",
    apiSpec: "claude_managed_agents",
  },
  {
    id: "openclaw",
    name: "OpenClaw Bridge",
    description: "OpenClaw Gateway, exposed through OAP's self-hosted open-harness protocol bridge.",
    repoPath: "templates/openclaw",
    repoUrl:
      "https://github.com/LiteLLM-Labs/litellm-agent-platform/tree/main/templates/openclaw",
    runtimeAlias: "openclaw",
    apiSpec: "claude_managed_agents",
  },
];

export function runtimeTemplateIconId(template: Pick<RuntimeTemplate, "id">): string {
  if (template.id === "deepagents") return "langchain";
  if (template.id === "opencode") return "opencode";
  if (template.id === "hermes") return "hermes";
  if (template.id === "openclaw") return "openclaw";
  return template.id;
}

export function runtimeTemplateById(
  id: string | null,
  templates: RuntimeTemplate[] = RUNTIME_TEMPLATES,
): RuntimeTemplate | null {
  if (!id) return null;
  return templates.find((template) => template.id === id) ?? null;
}

