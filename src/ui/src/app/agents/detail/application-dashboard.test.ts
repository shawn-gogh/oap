import { describe, expect, it } from "vitest";

import type { Agent } from "@/lib/types";
import {
  applicationContractFromAgent,
  dashboardDataFromArtifacts,
} from "./application-dashboard";

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
          outputs: [
            { type: "interactive_dashboard", description: "Review findings" },
          ],
          dashboard: {
            title: "发布审查大屏",
            description: "展示发布风险。",
            template: "executive",
            metrics: ["风险数"],
            dimensions: ["版本"],
            visualizations: ["指标卡"],
          },
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

describe("dashboardDataFromArtifacts", () => {
  it("reads dashboard data from a structured artifact", () => {
    const data = dashboardDataFromArtifacts([
      {
        id: "artifact-1",
        task_id: "task-1",
        attempt_number: 1,
        artifact_type: "dashboard",
        name: "分析结果",
        content_json: {
          metrics: { 销售额: 1280, 达标: true },
          rows: [{ 月份: "七月", 销售额: 1280 }],
        },
        created_by: "alice",
        created_at: 100,
      },
    ]);

    expect(data?.metrics).toEqual({ 销售额: 1280, 达标: true });
    expect(data?.rows).toEqual([{ 月份: "七月", 销售额: 1280 }]);
  });

  it("extracts JSON from a captured assistant text artifact", () => {
    const data = dashboardDataFromArtifacts([
      {
        id: "artifact-2",
        task_id: "task-1",
        attempt_number: 1,
        artifact_type: "session_output",
        name: "会话输出",
        content_json: {
          text: '结果如下：\n```json\n{"metrics":{"订单数":6},"rows":[{"渠道":"网页","订单数":6}]}\n```',
        },
        created_by: "system",
        created_at: 200,
      },
    ]);

    expect(data?.metrics).toEqual({ 订单数: 6 });
    expect(data?.rows[0]).toEqual({ 渠道: "网页", 订单数: 6 });
  });
});
