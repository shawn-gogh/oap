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
function toLapAgentBody({ name, description, system, tools }) {
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
    config: { runtime: "local-opencode" },
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

async function main() {
  const push = process.argv.includes("--push");
  mkdirSync(OUT, { recursive: true });
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
    const fields = { name, description: fm.description || "", system: body, tools };
    const yaml = toLapYaml(fields);
    writeFileSync(path.join(OUT, `${stem}.yaml`), yaml);

    let status = "yaml";
    if (push) {
      try {
        const created = await pushAgent(toLapAgentBody(fields));
        status = `pushed (${created.id ?? "ok"})`;
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
