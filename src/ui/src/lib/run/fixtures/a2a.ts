import { buildCompletedRunFixture } from "./shared";

export const a2aFixture = buildCompletedRunFixture({
  fixtureId: "a2a",
  providerName: "a2a",
  agentName: "A2A 示例智能体",
  toolLabel: "远程任务执行",
  resultText: "已通过 A2A 协议完成任务，远端返回了最终答案与一份报告文件。",
  artifact: {
    name: "report.md",
    mediaType: "text/markdown",
    inline: "# 执行报告\n\n任务已完成，详情见正文。",
  },
});
