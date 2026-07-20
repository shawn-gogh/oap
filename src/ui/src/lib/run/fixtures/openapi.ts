import { buildCompletedRunFixture } from "./shared";

export const openapiFixture = buildCompletedRunFixture({
  fixtureId: "openapi",
  providerName: "openapi",
  agentName: "OpenAPI 站内智能体",
  toolLabel: "调用站内接口",
  resultText: "已按确认的请求/响应字段映射调用站内接口，返回了结构化结果。",
  artifact: {
    name: "response.json",
    mediaType: "application/json",
    inline: { status: "ok", answer: "示例返回内容" },
  },
});
