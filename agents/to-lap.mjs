#!/usr/bin/env node
// Convert opencode file-based agents (agents/*.md, YAML frontmatter + markdown
// body) into LAP managed-agent YAML configs you can paste into the
// "Agent YAML config" editor (or POST to /api/agents).
//
//   node agents/to-lap.mjs            # writes agents/lap/<name>.yaml
//
// These .md agents live on disk (not in a running opencode server's store), so
// the one-click importer can't discover them; this maps them directly instead.

import { readFileSync, writeFileSync, readdirSync, mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const DIR = path.dirname(fileURLToPath(import.meta.url));
const OUT = path.join(DIR, "lap");

// opencode permission key -> LAP tool type(s). Only allow/ask are included.
const PERMISSION_TOOLS = {
  read: ["read"],
  edit: ["edit", "write"], // opencode "edit" covers create/modify files
  glob: ["glob"],
  grep: ["grep"],
  bash: ["bash"],
  webfetch: ["web_fetch"],
  websearch: ["web_search"],
  // "question" (ask the user) has no LAP tool equivalent — omitted.
};

function parseFrontmatter(text) {
  // Split the first --- ... --- block from the body.
  const m = text.match(/^---\n([\s\S]*?)\n---\n?([\s\S]*)$/);
  if (!m) return { fm: {}, body: text.trim() };
  const [, fmRaw, body] = m;
  const fm = {};
  let curKey = null;
  for (const line of fmRaw.split("\n")) {
    if (/^\s+\S/.test(line) && curKey) {
      // nested key: value (e.g. permission block)
      const mm = line.match(/^\s+([A-Za-z_]+):\s*(.+?)\s*$/);
      if (mm) (fm[curKey] ||= {})[mm[1]] = strip(mm[2]);
      continue;
    }
    const mm = line.match(/^([A-Za-z_]+):\s*(.*)$/);
    if (!mm) continue;
    curKey = mm[1];
    const val = mm[2].trim();
    fm[curKey] = val === "" ? {} : strip(val);
  }
  return { fm, body: body.trim() };
}

const strip = (v) => v.replace(/^["']|["']$/g, "").trim();

function toolsFromPermission(perm) {
  const tools = [];
  if (perm && typeof perm === "object") {
    for (const [key, val] of Object.entries(perm)) {
      if (val === "deny") continue;
      for (const t of PERMISSION_TOOLS[key] || []) {
        if (!tools.includes(t)) tools.push(t);
      }
    }
  }
  if (tools.length === 0) tools.push("read");
  return tools;
}

// Extract a short name: the part before " - " / " — " in the description,
// else the filename stem.
function deriveName(description, stem) {
  if (description) {
    const head = description.split(/\s[-—–]\s/)[0].trim();
    if (head && head.length <= 24) return head;
  }
  return stem;
}

function yamlScalar(s) {
  // Quote if it contains characters YAML would misparse.
  if (s === "") return '""';
  if (/[:#\-?{}\[\],&*!|>'"%@`]/.test(s) || /^\s|\s$/.test(s)) {
    return JSON.stringify(s);
  }
  return s;
}

function indentBody(body) {
  return body
    .split("\n")
    .map((l) => (l.length ? "  " + l : ""))
    .join("\n");
}

function toLapYaml({ name, description, system, tools }) {
  const lines = [
    `name: ${yamlScalar(name)}`,
    `description: ${yamlScalar(description || "")}`,
    `model: deepseek-chat`,
    `runtime: local-opencode`,
    `tools:`,
    ...tools.map((t) => `  - type: ${t}`),
    `system: |`,
    indentBody(system),
    `max_runtime_minutes: 30`,
    `on_failure: pause_and_notify`,
  ];
  return lines.join("\n") + "\n";
}

// Build the JSON body POST /api/agents expects (a CreateManagedAgent).
function toLapAgentBody({ name, description, system, tools, fm }) {
  const platform_mcp_ids = [];
  if (fm?.permission?.question === "allow") {
    platform_mcp_ids.push("request_human_approval");
  }
  return {
    name,
    owner_id: "local",
    description,
    runtime: "local-opencode",
    model: "deepseek-chat",
    system,
    prompt: system,
    tools: tools.map((type) => ({ type })),
    // config.runtime is the per-agent default runtime the session UI reads.
    config: {
      runtime: "local-opencode",
      ...(platform_mcp_ids.length > 0 ? { platform_mcp_ids } : {})
    },
    max_runtime_minutes: 30,
    on_failure: "pause_and_notify",
    vault_keys: [],
    skill_ids: [],
    rule_ids: [],
    setup_commands: [],
  };
}

async function pushAgent(body) {
  const base = (process.env.LAP_URL || "http://localhost:4000").replace(/\/+$/, "");
  const key = process.env.LAP_MASTER_KEY || "sk-local";
  const res = await fetch(`${base}/api/agents`, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      authorization: `Bearer ${key}`,
    },
    body: JSON.stringify(body),
  });
  const text = await res.text();
  if (!res.ok) throw new Error(`HTTP ${res.status}: ${text}`);
  return JSON.parse(text);
}

async function getAgents() {
  const base = (process.env.LAP_URL || "http://localhost:4000").replace(/\/+$/, "");
  const key = process.env.LAP_MASTER_KEY || "sk-local";
  try {
    const res = await fetch(`${base}/api/agents`, {
      headers: {
        authorization: `Bearer ${key}`,
      },
    });
    if (!res.ok) return [];
    const data = await res.json();
    return data.agents || [];
  } catch (e) {
    return [];
  }
}

async function updateAgent(id, body) {
  const base = (process.env.LAP_URL || "http://localhost:4000").replace(/\/+$/, "");
  const key = process.env.LAP_MASTER_KEY || "sk-local";
  // Remove 'tools' as it's not accepted in UpdateManagedAgent
  const { tools, ...updateBody } = body;
  const res = await fetch(`${base}/api/agents/${id}`, {
    method: "PATCH",
    headers: {
      "content-type": "application/json",
      authorization: `Bearer ${key}`,
    },
    body: JSON.stringify(updateBody),
  });
  const text = await res.text();
  if (!res.ok) throw new Error(`HTTP ${res.status}: ${text}`);
  return JSON.parse(text);
}

async function main() {
  const push = process.argv.includes("--push");
  mkdirSync(OUT, { recursive: true });
  let existingAgents = [];
  if (push) {
    existingAgents = await getAgents();
  }
  const files = readdirSync(DIR).filter(
    (f) => f.endsWith(".md") && f !== "README.md",
  );
  const summary = [];
  for (const file of files) {
    const stem = file.replace(/\.md$/, "");
    const text = readFileSync(path.join(DIR, file), "utf8");
    const { fm, body } = parseFrontmatter(text);
    if (!fm.description && !body) continue;
    const tools = toolsFromPermission(fm.permission);
    const name = deriveName(fm.description, stem);
    
    let system = body;
    if (fm.permission?.question === "allow") {
      system += "\n\n---\n### 人类审批与提问协议（必读）：\n由于运行在 LAP 平台定制化容器中，你实际调用的提问与审批工具并不是 `question`，而是带 `platform_` 前缀的远程 MCP 工具。当你需要向用户提问、确认或发起审批时，请遵循以下规范：\n\n1. **发起提问/审批**：调用 `platform_request_human_approval` 工具。\n   - `title`: 必填。展示给用户的核心问题，如“请选择要分析的区域”。\n   - `body`: 选填。提供问题的详细背景或上下文。\n   - `options`: 选填。一个字符串数组（如 `[\"选项A\", \"选项B\"]`），会在界面上渲染为直观的按钮，方便用户快速点击选择（强烈推荐）。\n   - `arguments`: 选填。在没有 `options` 时可以传入 `{ \"answer\": \"\" }` 供用户文字输入。\n   调用后，你将收到一个 `approval_id`。\n\n2. **等待与读取回答**：立即使用 `platform_check_human_approval` 并传入 `approval_id` 进行轮询查询，直到 status 不再是 `pending`：\n   - `pending`: 用户仍未答复，请继续等待（可在不需要此回答的前期步骤中继续处理，但获取结果前必须拿到答案）。\n   - `accepted`: 代表审批通过。如果此前提供了 `options`，用户的选择会保存在 `arguments.choice` 或 `arguments.selected_option` 中；若为文字回答，则保存在 `arguments.answer` 中。读取此结果并继续生成。\n   - `rejected`: 审批被驳回。从 `feedback` 中读取被拒绝的原因，调整方案或终止。";
    }
    
    const fields = { name, description: fm.description || "", system, tools, fm };
    const yaml = toLapYaml(fields);
    writeFileSync(path.join(OUT, `${stem}.yaml`), yaml);

    let status = "yaml";
    if (push) {
      try {
        const existing = existingAgents.find((a) => a.name === name);
        if (existing) {
          const updated = await updateAgent(existing.id, toLapAgentBody(fields));
          status = `updated (${updated.id ?? "ok"})`;
        } else {
          const created = await pushAgent(toLapAgentBody(fields));
          status = `pushed (${created.id ?? "ok"})`;
        }
      } catch (e) {
        status = `PUSH FAILED: ${e.message}`;
      }
    }
    summary.push({ file, name, tools: tools.join(","), status });
  }
  console.log(
    `converted ${summary.length} agent(s) -> agents/lap/${push ? " + pushed to LAP" : ""}`,
  );
  for (const s of summary) {
    console.log(`  ${s.file.padEnd(22)} name="${s.name}"  [${s.status}]`);
  }
  if (!push) {
    console.log(`\nto import into the running gateway:`);
    console.log(`  LAP_MASTER_KEY=sk-local node agents/to-lap.mjs --push`);
  }
}

main();
