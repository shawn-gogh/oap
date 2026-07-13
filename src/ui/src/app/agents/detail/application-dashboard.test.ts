import { describe, expect, it } from "vitest";

import type { Agent } from "@/lib/types";
import { applicationContractFromAgent } from "./application-dashboard";

describe("applicationContractFromAgent", () => {
  it("reads the persisted application contract", () => {
    const agent: Agent = {
      id: "reviewer",
      name: "Reviewer",
      config: {
        application: {
          version: 1,
          objective: "Review releases before deployment.",
          audience: ["release manager"],
          interaction_mode: "manual",
          inputs: [
            {
              type: "diff",
              source: "repository",
              description: "Release changes",
            },
          ],
          outputs: [{ type: "report", description: "Review findings" }],
          non_goals: ["Do not deploy"],
          completion_criteria: ["Every risk has evidence"],
          failure_behavior: "Report missing context.",
        },
      },
    };

    expect(applicationContractFromAgent(agent)).toEqual(
      agent.config?.application,
    );
  });

  it("returns null for legacy agents without a usable objective", () => {
    expect(
      applicationContractFromAgent({ id: "legacy", name: "Legacy" }),
    ).toBeNull();
    expect(
      applicationContractFromAgent({
        id: "empty",
        name: "Empty",
        config: { application: { objective: "  " } },
      }),
    ).toBeNull();
  });

  it("normalizes unknown interaction modes", () => {
    const contract = applicationContractFromAgent({
      id: "agent",
      name: "Agent",
      config: {
        application: { objective: "Do work", interaction_mode: "unexpected" },
      },
    });

    expect(contract?.interaction_mode).toBe("manual");
  });
});
