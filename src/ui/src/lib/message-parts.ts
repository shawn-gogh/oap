import type { HarnessMessagePart } from "@/lib/types";

export interface AssistantPartSections {
  activity: HarnessMessagePart[];
  reasoning: HarnessMessagePart[];
  response: HarnessMessagePart[];
}

export function partitionAssistantParts(
  parts: HarnessMessagePart[],
  completed: boolean,
): AssistantPartSections {
  const reasoning: HarnessMessagePart[] = [];
  const visible = parts.filter((part) => {
    if (part.type === "thinking" || part.type === "reasoning") {
      reasoning.push(part);
      return false;
    }
    return true;
  });
  const lastToolIndex = visible.findLastIndex((part) => part.type === "tool");
  if (lastToolIndex < 0) {
    return { activity: [], reasoning, response: visible };
  }
  if (!completed) {
    return { activity: visible, reasoning, response: [] };
  }
  return {
    activity: visible.slice(0, lastToolIndex + 1),
    reasoning,
    response: visible.slice(lastToolIndex + 1),
  };
}
