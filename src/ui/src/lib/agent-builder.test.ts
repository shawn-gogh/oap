import { describe, expect, it } from "vitest";

import {
  applicationGatePassed,
  blankAgentDraft,
  blankDesign,
  createInputFromDraft,
  evalGatePassed,
  parseAgentDraftConfig,
  stringifyAgentDraft,
} from "./agent-builder";
import type { AgentDesign, AgentDraft } from "./agent-builder";

function fullDraft(): AgentDraft {
  const design: AgentDesign = {
    feasibility: {
      complexity: true,
      value: true,
      model_fit: false,
      recoverable_errors: true,
    },
    evaluation: {
      task_distribution: [{ type: "primary", example: "summarize a report" }],
      success_criteria: "Output is a correct, reviewable summary.",
      normal_cases: ["clear request with full context"],
      edge_cases: ["ambiguous request"],
      recovery_cases: ["tool unavailable"],
      safety_cases: ["asks for destructive action"],
      evaluator: "rule",
    },
    governance: {
      write_requires_approval: true,
      credential_isolation: true,
      tool_whitelist: true,
      timeout_minutes: 45,
    },
  };
  return {
    ...blankAgentDraft(),
    name: "security-reviewer",
    description: "Reviews diffs for security issues: injection, secrets, authz.",
    model: "anthropic/claude-sonnet-4-6",
    runtime: "claude_managed_agents",
    system: "You are a meticulous reviewer.\n\nRules:\n- never approve secrets\n- cite line numbers",
    tools: [{ type: "read" }, { type: "grep" }, { type: "bash" }],
    cron: "0 9 * * 1",
    timezone: "Asia/Shanghai",
    vault_keys: ["GH_TOKEN", "SLACK_WEBHOOK"],
    skill_ids: ["skill_a", "skill_b"],
    rule_ids: ["rule_x"],
    sub_agents: [{ agent_id: "agent_123" }],
    mcp_server_ids: ["mcp_github"],
    max_runtime_minutes: 45,
    application: {
      version: 1,
      objective: "Review code changes and produce evidence-backed security findings.",
      audience: ["security reviewer", "pull request author"],
      interaction_mode: "manual",
      inputs: [
        {
          type: "diff",
          source: "repository",
          description: "The proposed code changes.",
        },
      ],
      outputs: [
        {
          type: "review",
          description: "Severity-ranked findings with file references.",
        },
      ],
      non_goals: ["Do not modify the repository."],
      completion_criteria: ["Every finding includes evidence and a severity."],
      failure_behavior: "Report missing repository context and stop.",
    },
    design,
  };
}

describe("stringify/parse round trip", () => {
  it("preserves every field of a fully populated draft", () => {
    const draft = fullDraft();
    const { draft: parsed, error } = parseAgentDraftConfig(stringifyAgentDraft(draft));
    expect(error).toBeNull();
    expect(parsed.name).toBe(draft.name);
    expect(parsed.description).toBe(draft.description);
    expect(parsed.model).toBe(draft.model);
    expect(parsed.runtime).toBe(draft.runtime);
    expect(parsed.system).toBe(draft.system);
    expect(parsed.tools).toEqual(draft.tools);
    expect(parsed.cron).toBe(draft.cron);
    expect(parsed.timezone).toBe(draft.timezone);
    expect(parsed.vault_keys).toEqual(draft.vault_keys);
    expect(parsed.skill_ids).toEqual(draft.skill_ids);
    expect(parsed.rule_ids).toEqual(draft.rule_ids);
    expect(parsed.sub_agents).toEqual(draft.sub_agents);
    expect(parsed.mcp_server_ids).toEqual(draft.mcp_server_ids);
    expect(parsed.max_runtime_minutes).toBe(45);
    expect(parsed.application).toEqual(draft.application);
  });

  it("round-trips the design block with correct scalar types", () => {
    const { draft: parsed } = parseAgentDraftConfig(stringifyAgentDraft(fullDraft()));
    expect(parsed.design?.feasibility).toEqual({
      complexity: true,
      value: true,
      model_fit: false,
      recoverable_errors: true,
    });
    expect(parsed.design?.evaluation?.success_criteria).toBe("Output is a correct, reviewable summary.");
    expect(parsed.design?.evaluation?.normal_cases).toEqual(["clear request with full context"]);
    expect(parsed.design?.evaluation?.safety_cases).toEqual(["asks for destructive action"]);
    expect(parsed.design?.governance?.timeout_minutes).toBe(45);
    expect(parsed.design?.governance?.write_requires_approval).toBe(true);
  });

  it("persists the application contract in the create payload", () => {
    const draft = fullDraft();
    const input = createInputFromDraft(draft, []);
    expect(input.config.application).toEqual(draft.application);
  });

  it("keeps legacy configs without an application contract compatible", () => {
    const { draft, error } = parseAgentDraftConfig("name: legacy\nmodel: m\nruntime: r\nsystem: s");
    expect(error).toBeNull();
    expect(draft.application).toBeUndefined();
  });

  it("round-trips values that need quoting", () => {
    const draft = {
      ...fullDraft(),
      name: "agent: with colon",
      description: "says \"hello\" and uses 'quotes'",
    };
    const { draft: parsed, error } = parseAgentDraftConfig(stringifyAgentDraft(draft));
    expect(error).toBeNull();
    expect(parsed.name).toBe(draft.name);
    expect(parsed.description).toBe(draft.description);
  });

  it("round-trips an empty tools list", () => {
    const draft = { ...fullDraft(), tools: [] };
    const { draft: parsed } = parseAgentDraftConfig(stringifyAgentDraft(draft));
    expect(parsed.tools).toEqual([]);
  });
});

describe("parseAgentDraftConfig validation", () => {
  it("requires a name", () => {
    const { error } = parseAgentDraftConfig("name: \nmodel: m\nruntime: r\nsystem: s");
    expect(error).toBe("Agent name is required.");
  });

  it("requires a model", () => {
    const { error } = parseAgentDraftConfig("name: a\nmodel: \nruntime: r\nsystem: s");
    expect(error).toBe("Model is required.");
  });

  it("requires a runtime", () => {
    const { error } = parseAgentDraftConfig("name: a\nmodel: m\nruntime: \nsystem: s");
    expect(error).toBe("Runtime is required.");
  });

  it("reports the line it cannot parse", () => {
    const { error } = parseAgentDraftConfig("name: a\n???\nmodel: m");
    expect(error).toBe("Could not parse line 2.");
  });
});

describe("evalGatePassed", () => {
  it("fails without an evaluation", () => {
    expect(evalGatePassed(undefined)).toBe(false);
    expect(evalGatePassed({})).toBe(false);
  });

  it("fails when any case category is empty", () => {
    const design = blankDesign();
    design.evaluation = {
      task_distribution: [],
      success_criteria: "works",
      normal_cases: ["n"],
      edge_cases: ["e"],
      recovery_cases: [],
      safety_cases: ["s"],
      evaluator: "rule",
    };
    expect(evalGatePassed(design)).toBe(false);
  });

  it("passes with criteria and one case per category", () => {
    const design = blankDesign();
    design.evaluation = {
      task_distribution: [],
      success_criteria: "works",
      normal_cases: ["n"],
      edge_cases: ["e"],
      recovery_cases: ["r"],
      safety_cases: ["s"],
      evaluator: "rule",
    };
    expect(evalGatePassed(design)).toBe(true);
  });
});

describe("applicationGatePassed", () => {
  it("requires an objective, input, output, and completion criterion", () => {
    const application = fullDraft().application!;
    expect(applicationGatePassed(application)).toBe(true);
    expect(applicationGatePassed({ ...application, inputs: [] })).toBe(false);
    expect(applicationGatePassed({ ...application, outputs: [] })).toBe(false);
    expect(applicationGatePassed({ ...application, completion_criteria: [] })).toBe(false);
    expect(applicationGatePassed({ ...application, objective: "" })).toBe(false);
  });
});
