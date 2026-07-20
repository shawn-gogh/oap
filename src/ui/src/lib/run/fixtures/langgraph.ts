import { buildCompletedRunFixture } from "./shared";

export const langgraphFixture = buildCompletedRunFixture({
  fixtureId: "langgraph",
  providerName: "langgraph",
  agentName: "LangGraph 研究图",
  toolLabel: "运行图节点",
  resultText: "LangGraph 图运行至终止状态，已按确认的输入/输出映射取回最终答案。",
  artifact: {
    name: "trace.json",
    mediaType: "application/json",
    inline: { assistant_id: "runtime-fallback-test", state: "completed" },
  },
});
