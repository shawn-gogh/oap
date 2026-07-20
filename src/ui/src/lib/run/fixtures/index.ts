import type { ControlEventV1, RunSnapshotV1 } from "../types";
import { a2aFixture } from "./a2a";
import { crewaiFixture } from "./crewai";
import { difyFixture } from "./dify";
import { langgraphFixture } from "./langgraph";
import { openapiFixture } from "./openapi";
import { failedFixture, waitingApprovalFixture, waitingInputFixture } from "./scenarios";

export interface RunFixtureEntry {
  id: string;
  label: string;
  snapshot: RunSnapshotV1;
  events: ControlEventV1[];
}

export const ALL_FIXTURES: Record<string, RunFixtureEntry> = {
  a2a: { id: "a2a", label: "A2A（已完成）", ...a2aFixture },
  openapi: { id: "openapi", label: "OpenAPI（已完成）", ...openapiFixture },
  langgraph: { id: "langgraph", label: "LangGraph（已完成）", ...langgraphFixture },
  dify: { id: "dify", label: "Dify（已完成）", ...difyFixture },
  crewai: { id: "crewai", label: "CrewAI（已完成）", ...crewaiFixture },
  waiting_approval: { id: "waiting_approval", label: "场景：等待审批", ...waitingApprovalFixture },
  waiting_input: { id: "waiting_input", label: "场景：等待补充输入", ...waitingInputFixture },
  failed: { id: "failed", label: "场景：执行失败", ...failedFixture },
};

export const FIXTURE_IDS = Object.keys(ALL_FIXTURES);
