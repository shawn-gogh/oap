import { buildCompletedRunFixture } from "./shared";

export const crewaiFixture = buildCompletedRunFixture({
  fixtureId: "crewai",
  providerName: "crewai",
  agentName: "CrewAI 研究小组",
  toolLabel: "kickoff 执行",
  resultText: "CrewAI 小组已完成 kickoff 任务，按确认的输入映射得到最终结果。",
  artifact: {
    name: "crew-result.txt",
    mediaType: "text/plain",
    inline: "任务完成，结果摘要见正文。",
  },
});
