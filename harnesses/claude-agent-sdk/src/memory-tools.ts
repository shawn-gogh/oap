/**
 * Claude-Agent-SDK adapter for the shared managed-tools/memory spec.
 *
 * All the real logic — schemas, descriptions, HTTP client — lives in
 * `@lap/managed-tools/memory`. This file's only job is to bridge that
 * spec to the Claude Agent SDK's tool API (`tool()` + `createSdkMcpServer`).
 *
 * When a future harness (e.g. opencode) wants memory, it imports from
 * `@lap/managed-tools/memory` and writes its own ~40-line adapter the
 * same way.
 */

import {
  createSdkMcpServer,
  tool,
  type McpSdkServerConfigWithInstance,
} from "@anthropic-ai/claude-agent-sdk";
import {
  callSaveMemory,
  callSearchMemory,
  memoryEnv,
  saveMemoryDescription,
  saveMemorySchema,
  searchMemoryDescription,
  searchMemorySchema,
  type SaveMemoryInput,
  type SearchMemoryInput,
} from "@lap/managed-tools/memory";
import {
  callReportPreviewUrl,
  reportPreviewUrlDescription,
  reportPreviewUrlSchema,
  type ReportPreviewUrlInput,
} from "@lap/managed-tools/preview";

export function buildMemoryMcpServer(opts?: {
  onPreviewPortRegistered?: (port: number) => void;
}): McpSdkServerConfigWithInstance | null {
  const env = memoryEnv();
  if (!env) return null;

  const saveMemory = tool(
    "save_memory",
    saveMemoryDescription,
    saveMemorySchema,
    async (input: SaveMemoryInput) => {
      const out = await callSaveMemory(env, input, {
        source_session_id: process.env.SESSION_ID || undefined,
      });
      return {
        content: [{ type: "text" as const, text: out.text }],
        ...(out.isError && { isError: true }),
      };
    },
  );

  const searchMemory = tool(
    "search_memory",
    searchMemoryDescription,
    searchMemorySchema,
    async (input: SearchMemoryInput) => {
      const out = await callSearchMemory(env, input);
      return {
        content: [{ type: "text" as const, text: out.text }],
        ...(out.isError && { isError: true }),
      };
    },
  );

  const reportPreviewUrl = tool(
    "report_preview_url",
    reportPreviewUrlDescription,
    reportPreviewUrlSchema,
    async (input: ReportPreviewUrlInput) => {
      const out = await callReportPreviewUrl(env, input);
      if (!out.isError) {
        opts?.onPreviewPortRegistered?.(input.port);
      }
      return {
        content: [{ type: "text" as const, text: out.text }],
        ...(out.isError && { isError: true }),
      };
    },
  );

  return createSdkMcpServer({
    name: "lap-memory",
    version: "0.1.0",
    tools: [saveMemory, searchMemory, reportPreviewUrl],
  });
}

export const MEMORY_TOOL_NAMES = [
  "mcp__lap-memory__save_memory",
  "mcp__lap-memory__search_memory",
  "mcp__lap-memory__report_preview_url",
] as const;
