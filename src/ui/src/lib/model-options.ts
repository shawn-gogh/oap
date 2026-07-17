export function modelOptions(models: string[], currentModel: string): string[] {
  const options = normalizeModels(models);
  if (options.length > 0) return options;
  const current = currentModel.trim();
  return current ? [current] : [];
}

// Federated bridge runtimes (see src/http/sessions/external_bridge.rs) never
// register a model list — they execute through a direct provider-specific
// bridge, not a managed runtime harness, so GET /api/models?runtime=... 400s
// with "unsupported runtime" for them. Skip discovery and use each
// provider's fixed default model instead, mirroring the server-side
// `ImportAgentsProvider::default_model()` for that provider.
const FEDERATED_BRIDGE_RUNTIMES: Record<string, string> = {
  a2a_v1: "a2a-remote",
  acp_legacy: "acp-remote",
  dify_app: "dify-managed",
  openapi_rest: "openapi-mapped",
};

export function isFederatedBridgeRuntime(runtime?: string | null): boolean {
  return Boolean(runtime && runtime in FEDERATED_BRIDGE_RUNTIMES);
}

export function runtimeSupportsModelDiscovery(runtime?: string | null): boolean {
  if (!runtime) return true;
  return runtime !== "elastic_agent_builder" && !isFederatedBridgeRuntime(runtime);
}

export function defaultModelForRuntime(runtime?: string | null): string {
  if (!runtime) return "";
  if (runtime === "elastic_agent_builder") return "elastic-agent-builder";
  return FEDERATED_BRIDGE_RUNTIMES[runtime] ?? "";
}

export function selectedRuntimeModel(models: string[], currentModel: string): string {
  const options = normalizeModels(models);
  const current = currentModel.trim();
  if (current && options.includes(current)) return current;
  return preferredModel(options);
}

function normalizeModels(models: string[]): string[] {
  return [...new Set(models.map((model) => model.trim()).filter(Boolean))];
}

export function preferredModel(models: string[]): string {
  const options = normalizeModels(models);
  const concrete = options.filter((model) => !model.endsWith("/*"));
  const preferred = concrete
    .map((model, index) => ({ model, index, score: defaultModelScore(model) }))
    .filter((entry) => entry.score > 0)
    .sort((a, b) => b.score - a.score || a.index - b.index)[0];
  return (
    preferred?.model ??
    concrete.find((model) => !isDiscouragedDefaultModel(model)) ??
    concrete[0] ??
    options[0] ??
    ""
  );
}

function defaultModelScore(model: string): number {
  const normalized = model.toLowerCase();
  if (isDiscouragedDefaultModel(normalized) || normalized.endsWith("/*")) return 0;
  if (/(^|\/)claude-sonnet-4(?:[-.]|$)/.test(normalized)) return 600_000 + versionScore(normalized);
  if (/(^|\/)claude-opus-4(?:[-.]|$)/.test(normalized)) return 500_000 + versionScore(normalized);
  if (/(^|\/)claude-haiku-4(?:[-.]|$)/.test(normalized)) return 400_000 + versionScore(normalized);
  if (/(^|\/)claude-4(?:[-.]|$)/.test(normalized)) return 350_000 + versionScore(normalized);
  if (/(^|\/)gpt-5(?:[-.]|$)/.test(normalized)) return 300_000 + versionScore(normalized);
  if (/(^|\/)claude-(?:3[-.]7|3[-.]5)-sonnet(?:[-.]|$)/.test(normalized)) {
    return 200_000 + versionScore(normalized);
  }
  return 0;
}

function isDiscouragedDefaultModel(model: string): boolean {
  return /(^|\/)claude-(?:fable|mythos)-5(?:[-.]|$)/.test(model.toLowerCase());
}

function versionScore(model: string): number {
  const stablePrefix = model.replace(/-\d{8}\b.*$/, "");
  return Array.from(stablePrefix.matchAll(/\d+/g))
    .slice(0, 3)
    .map((match) => Number(match[0]))
    .reduce((score, value) => score * 100 + value, 0);
}
