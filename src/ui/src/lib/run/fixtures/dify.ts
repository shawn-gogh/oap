import { buildCompletedRunFixture } from "./shared";

export const difyFixture = buildCompletedRunFixture({
  fixtureId: "dify",
  providerName: "dify",
  agentName: "Dify 工作流应用",
  toolLabel: "工作流节点",
  resultText: "Dify 工作流已执行完毕，按确认的输入映射得到最终输出。",
  artifact: {
    name: "workflow-output.csv",
    mediaType: "text/csv",
    inline: "field,value\nanswer,示例输出",
  },
});
