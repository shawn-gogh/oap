#!/usr/bin/env node
/**
 * Standalone stdio MCP server exposing `provision` + `execute` sandbox tools
 * for the opencode harness.
 *
 * Delegates to the LAP platform API — same endpoints as claude-agent-sdk's
 * buildSandboxMcpServer. The platform owns template selection, vault proxy
 * injection, and sandbox lifecycle.
 *
 * SESSION_ID: read from env (K8s per-session pod) or from tool args (inline
 * shared Render service where no per-session env injection exists).
 *
 * Requires: LAP_BASE_URL, LAP_AUTH_TOKEN/MASTER_KEY in env.
 */

import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";

const BASE = process.env.LAP_BASE_URL;
const ENV_SESSION_ID = process.env.SESSION_ID;
const TOKEN = process.env.LAP_AUTH_TOKEN ?? process.env.MASTER_KEY;

const server = new Server(
  { name: "opencode-sandbox", version: "1.0.0" },
  { capabilities: { tools: {} } },
);

const TOOLS = [
  {
    name: "provision",
    description:
      "Provision a new sandbox environment. Returns a confirmation message when the sandbox is ready. Use the chosen name as sandbox_name in subsequent execute() calls.",
    inputSchema: {
      type: "object",
      properties: {
        name: {
          type: "string",
          description: "Label for the sandbox — used in subsequent execute() calls as sandbox_name",
        },
        project_id: {
          type: "string",
          description: "ID of the project template to provision the sandbox from",
        },
        session_id: {
          type: "string",
          description: "LAP session ID — required when SESSION_ID env var is not set",
        },
      },
      required: ["name"],
    },
  },
  {
    name: "execute",
    description:
      "Execute a shell command inside a provisioned sandbox. Returns the command output.",
    inputSchema: {
      type: "object",
      properties: {
        sandbox_name: {
          type: "string",
          description: "Label of the provisioned sandbox to run the command in",
        },
        cmd: { type: "string", description: "Shell command to execute inside the sandbox" },
        session_id: {
          type: "string",
          description: "LAP session ID — required when SESSION_ID env var is not set",
        },
      },
      required: ["sandbox_name", "cmd"],
    },
  },
];

server.setRequestHandler(ListToolsRequestSchema, async () => ({ tools: TOOLS }));

function textResult(text, isError = false) {
  return { content: [{ type: "text", text }], isError };
}

function resolveSession(args) {
  return ENV_SESSION_ID ?? args.session_id ?? null;
}

function missingConfig(session_id) {
  const missing = [
    !BASE && "LAP_BASE_URL",
    !session_id && "SESSION_ID",
    !TOKEN && "LAP_AUTH_TOKEN/MASTER_KEY",
  ].filter(Boolean);
  return missing.length ? `sandbox tools unavailable: missing ${missing.join(", ")}` : null;
}

async function provision({ name, project_id, session_id: argSession }) {
  const session_id = resolveSession({ session_id: argSession });
  const err = missingConfig(session_id);
  if (err) return textResult(`provision failed: ${err}`, true);
  try {
    const res = await fetch(
      `${BASE}/api/v1/managed_agents/sessions/${session_id}/sandbox/provision`,
      {
        method: "POST",
        headers: { "Content-Type": "application/json", Authorization: `Bearer ${TOKEN}` },
        body: JSON.stringify({ name, project_id }),
      },
    );
    const json = await res.json();
    if (!res.ok) return textResult(`provision failed: ${json.error ?? `HTTP ${res.status}`}`, true);
    return textResult(json.message ?? "sandbox provisioned");
  } catch (e) {
    return textResult(`provision error: ${e instanceof Error ? e.message : String(e)}`, true);
  }
}

async function execute({ sandbox_name, cmd, session_id: argSession }) {
  const session_id = resolveSession({ session_id: argSession });
  const err = missingConfig(session_id);
  if (err) return textResult(`execute failed: ${err}`, true);
  try {
    const res = await fetch(
      `${BASE}/api/v1/managed_agents/sessions/${session_id}/sandbox/execute`,
      {
        method: "POST",
        headers: { "Content-Type": "application/json", Authorization: `Bearer ${TOKEN}` },
        body: JSON.stringify({ sandbox_name, cmd }),
      },
    );
    const json = await res.json();
    if (!res.ok) return textResult(`execute failed: ${json.error ?? `HTTP ${res.status}`}`, true);
    return textResult(json.output ?? "");
  } catch (e) {
    return textResult(`execute error: ${e instanceof Error ? e.message : String(e)}`, true);
  }
}

server.setRequestHandler(CallToolRequestSchema, async (req) => {
  const { name, arguments: args } = req.params;
  if (name === "provision") return provision(args ?? {});
  if (name === "execute") return execute(args ?? {});
  return textResult(`unknown tool: ${name}`, true);
});

const transport = new StdioServerTransport();
await server.connect(transport);
console.error(
  `[sandbox-mcp] ready (session=${ENV_SESSION_ID ?? "from-args"}, base=${BASE ?? "MISSING"})`,
);
