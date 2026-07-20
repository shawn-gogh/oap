export function runtimeBrandIconId(alias: string, apiSpec?: string | null): string {
  const normalizedAlias = alias.toLowerCase();
  const normalizedSpec = (apiSpec ?? "").toLowerCase();
  const search = `${normalizedAlias} ${normalizedSpec}`;

  if (search.includes("dify")) return "dify";
  if (search.includes("langgraph")) return "langgraph";
  if (search.includes("crewai") || search.includes("crew-ai")) return "crewai";
  if (search.includes("a2a") || search.includes("agent-to-agent")) return "a2a";
  if (search.includes("openapi") || search.includes("rest")) return "openapi";
  if (search.includes("deepagents") || search.includes("deep-agents") || search.includes("langchain")) {
    return "langchain";
  }
  if (search.includes("hermes")) return "hermes";
  if (search.includes("opencode") || search.includes("open-code")) return "opencode";
  if (normalizedAlias === "claude_managed_agents" || normalizedSpec === "claude_managed_agents") return "claude";
  if (normalizedAlias === "elastic" || normalizedSpec === "elastic_agent_builder") return "elastic";
  if (normalizedAlias === "gemini_antigravity" || normalizedSpec === "gemini_antigravity") return "gemini";
  if (normalizedAlias === "cursor" || normalizedSpec === "cursor") return "cursor";

  return alias || apiSpec || "opencode";
}
