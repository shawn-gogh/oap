import type { AgentRuntime, RuntimeHarness } from "@/lib/types";

export function selectableAgentRuntimes(
  runtimes: AgentRuntime[],
  harnesses: RuntimeHarness[],
  currentRuntime = "",
): AgentRuntime[] {
  const options = new Map(runtimes.map((runtime) => [runtime.id, runtime]));
  for (const harness of harnesses) {
    if (harness.is_default || (!harness.connected && harness.alias !== currentRuntime)) continue;
    options.set(harness.alias, {
      id: harness.alias,
      name: harness.display_name,
      default_api_base: harness.api_base,
      credential_provider_id: harness.alias,
      credential_provider_name: harness.display_name,
      tools: harness.tools,
      approval_enforcement: harness.approval_enforcement,
      connected: harness.connected,
      api_base: harness.api_base,
      masked_api_key: harness.masked_api_key,
    });
  }
  if (currentRuntime && !options.has(currentRuntime)) {
    options.set(currentRuntime, {
      id: currentRuntime,
      name: currentRuntime === "local-opencode" ? "opencode（本地）" : currentRuntime,
      default_api_base: "",
      credential_provider_id: currentRuntime,
      credential_provider_name: currentRuntime,
      tools: [],
      connected: false,
    });
  }
  if (options.size === 0) {
    options.set("local-opencode", {
      id: "local-opencode",
      name: "opencode（本地）",
      default_api_base: "",
      credential_provider_id: "local-opencode",
      credential_provider_name: "opencode",
      tools: [],
      connected: false,
    });
  }
  return [...options.values()];
}
