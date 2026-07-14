import { describe, expect, it } from "vitest";
import type { AgentRuntime, RuntimeHarness } from "@/lib/types";
import { selectableAgentRuntimes } from "./agent-runtime-options";

const elastic: AgentRuntime = {
  id: "elastic_agent_builder",
  name: "Elastic Agent Builder",
  default_api_base: "",
  credential_provider_id: "elastic",
  credential_provider_name: "Elastic",
  tools: [],
  connected: true,
};

function harness(overrides: Partial<RuntimeHarness> = {}): RuntimeHarness {
  return {
    alias: "local-opencode",
    api_spec: "claude_managed_agents",
    display_name: "local-opencode",
    api_base: "http://opencode:8080",
    is_default: false,
    connected: true,
    tools: [],
    ...overrides,
  };
}

describe("selectableAgentRuntimes", () => {
  it("合并内置运行时和已连接的自定义运行时", () => {
    expect(selectableAgentRuntimes([elastic], [harness()]).map((item) => item.id)).toEqual([
      "elastic_agent_builder",
      "local-opencode",
    ]);
  });

  it("不把默认运行时目录重复加入自定义选项", () => {
    expect(
      selectableAgentRuntimes([elastic], [harness({ alias: elastic.id, is_default: true })]),
    ).toEqual([elastic]);
  });

  it("保留智能体当前使用但已断开的运行时", () => {
    const options = selectableAgentRuntimes([], [harness({ connected: false })], "local-opencode");
    expect(options).toHaveLength(1);
    expect(options[0]).toMatchObject({ id: "local-opencode", connected: false });
  });
});
