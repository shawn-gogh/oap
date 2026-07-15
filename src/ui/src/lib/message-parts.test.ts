import { describe, expect, it } from "vitest";

import type { HarnessMessagePart } from "@/lib/types";
import { partitionAssistantParts } from "./message-parts";

const text = (value: string): HarnessMessagePart => ({ type: "text", text: value });
const tool = (name: string): HarnessMessagePart => ({
  type: "tool",
  tool: name,
  state: { status: "completed" },
});

describe("partitionAssistantParts", () => {
  it("keeps tool narration in activity and the trailing summary as the response", () => {
    const sections = partitionAssistantParts([
      text("先检查入口"),
      tool("read"),
      text("继续检查配置"),
      tool("bash"),
      text("最终结果"),
    ], true);

    expect(sections.activity).toHaveLength(4);
    expect(sections.response).toEqual([text("最终结果")]);
  });

  it("does not promote streaming narration before the turn is terminal", () => {
    const sections = partitionAssistantParts([text("正在检查"), tool("bash"), text("继续验证")], false);
    expect(sections.activity).toHaveLength(3);
    expect(sections.response).toHaveLength(0);
  });

  it("renders a tool-free answer normally and separates reasoning", () => {
    const reasoning: HarnessMessagePart = { type: "thinking", text: "内部推理" };
    const sections = partitionAssistantParts([reasoning, text("直接回答")], true);
    expect(sections.reasoning).toEqual([reasoning]);
    expect(sections.response).toEqual([text("直接回答")]);
    expect(sections.activity).toHaveLength(0);
  });
});
