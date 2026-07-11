// External-agent adapter scaffold: implements the managed-agents runtime
// contract the gateway speaks (same surface as templates/deepagents), backed
// by in-memory state. Integrators only edit hooks.mjs.
//
// Contract endpoints:
//   GET  /health                          GET  /v1/models
//   POST /v1/agents   GET /v1/agents      GET  /v1/agents/:id
//   POST /v1/environments                 POST /v1/sessions
//   POST /v1/sessions/:id/events          GET  /v1/sessions/:id/events
//   GET  /v1/sessions/:id/events/stream   POST /v1/sessions/:id/abort

import express from "express";
import crypto from "node:crypto";
import { createRemoteSession, sendPrompt, abortRun, healthy } from "./hooks.mjs";

const PORT = Number(process.env.PORT || 8080);
const RUNTIME_API_KEY = process.env.RUNTIME_API_KEY || "";
const DEFAULT_MODEL = process.env.DEFAULT_MODEL || "external";

const app = express();
app.use(express.json({ limit: "10mb" }));

// ── auth ─────────────────────────────────────────────────────────────────────
app.use((req, res, next) => {
  if (req.path === "/health") return next();
  if (!RUNTIME_API_KEY) return next();
  const bearer = (req.headers.authorization || "").replace(/^Bearer\s+/i, "");
  const key = req.headers["x-api-key"] || bearer;
  if (key === RUNTIME_API_KEY) return next();
  res.status(401).json({ error: { message: "invalid runtime api key" } });
});

// ── in-memory state ──────────────────────────────────────────────────────────
const id = (prefix) => `${prefix}_${crypto.randomUUID().replaceAll("-", "")}`;
const agents = new Map(); // agentId -> {id, name, system, model, ...}
const sessions = new Map(); // sessionId -> {id, agentId, status, remoteState, events: [], history: [], listeners: Set}

function appendEvent(session, type, data) {
  const event = { id: session.events.length + 1, type, ...data };
  session.events.push(event);
  for (const listener of session.listeners) listener(event);
  return event;
}

const sseFrame = (event) =>
  `event: ${event.type}\ndata: ${JSON.stringify(event)}\n\n`;

// ── contract ─────────────────────────────────────────────────────────────────
app.get("/health", async (_req, res) => {
  const ok = await healthy().catch(() => false);
  res.status(ok ? 200 : 503).json({ ok });
});

app.get("/v1/models", (_req, res) => {
  res.json({ data: [{ id: DEFAULT_MODEL, display_name: DEFAULT_MODEL }] });
});

app.post("/v1/agents", (req, res) => {
  const agent = {
    id: id("agt"),
    name: req.body?.name || "external agent",
    system: req.body?.system_prompt || req.body?.system || "",
    model: req.body?.model?.model || req.body?.model || DEFAULT_MODEL,
  };
  agents.set(agent.id, agent);
  res.status(201).json(agent);
});

app.get("/v1/agents", (_req, res) => res.json({ data: [...agents.values()] }));

app.get("/v1/agents/:id", (req, res) => {
  const agent = agents.get(req.params.id);
  if (!agent) return res.status(404).json({ error: { message: "agent not found" } });
  res.json(agent);
});

app.post("/v1/environments", (req, res) => {
  res.status(201).json({ id: id("env"), agent_id: req.body?.agent_id ?? null });
});

app.post("/v1/sessions", async (req, res) => {
  const agent = agents.get(req.body?.agent_id) ?? null;
  const session = {
    id: id("ses"),
    agentId: agent?.id ?? null,
    status: "idle",
    remoteState: null,
    events: [],
    history: [],
    listeners: new Set(),
  };
  try {
    session.remoteState = await createRemoteSession({
      sessionId: session.id,
      agent,
      metadata: req.body?.metadata ?? {},
    });
  } catch (error) {
    return res.status(502).json({ error: { message: `createRemoteSession failed: ${error}` } });
  }
  sessions.set(session.id, session);
  res.status(201).json({ id: session.id, status: session.status });
});

function userText(events) {
  const chunks = [];
  for (const event of events ?? []) {
    if (event?.type !== "user.message") continue;
    const content = event.content;
    if (typeof content === "string") chunks.push(content);
    else if (Array.isArray(content)) {
      for (const item of content) {
        if (typeof item === "string") chunks.push(item);
        else if (item?.text) chunks.push(item.text);
      }
    }
  }
  return chunks.join("\n").trim();
}

app.post("/v1/sessions/:id/events", (req, res) => {
  const session = sessions.get(req.params.id);
  if (!session) return res.status(404).json({ error: { message: "session not found" } });
  const prompt = userText(req.body?.events);
  if (!prompt) return res.status(400).json({ error: { message: "no user.message text" } });
  if (session.status === "running") {
    return res.status(202).json({ ok: true, queued: false, busy: true });
  }
  session.status = "running";
  session.history.push({ role: "user", text: prompt });
  appendEvent(session, "user.message", { content: [{ type: "text", text: prompt }] });

  const agent = session.agentId ? agents.get(session.agentId) : null;
  (async () => {
    const emitted = [];
    const emit = (text) => {
      if (!text) return;
      emitted.push(text);
      session.history.push({ role: "assistant", text });
      appendEvent(session, "agent.message", {
        content: [{ type: "text", text }],
        model: agent?.model ?? DEFAULT_MODEL,
      });
    };
    try {
      const reply = await sendPrompt({
        sessionId: session.id,
        remoteState: session.remoteState,
        agent,
        prompt,
        history: session.history.slice(0, -1),
        emit,
      });
      if (typeof reply === "string" && reply.trim()) emit(reply);
      if (emitted.length === 0) emit("(external agent returned no text)");
      appendEvent(session, "session.status_idle", { stop_reason: { type: "end_turn" } });
      session.status = "idle";
    } catch (error) {
      appendEvent(session, "session.error", { error: { message: String(error) } });
      session.status = "error";
    }
  })();

  res.status(202).json({ ok: true, queued: false });
});

app.get("/v1/sessions/:id/events", (req, res) => {
  const session = sessions.get(req.params.id);
  if (!session) return res.status(404).json({ error: { message: "session not found" } });
  res.json({ data: session.events });
});

app.get("/v1/sessions/:id/events/stream", (req, res) => {
  const session = sessions.get(req.params.id);
  if (!session) return res.status(404).json({ error: { message: "session not found" } });
  res.writeHead(200, {
    "content-type": "text/event-stream",
    "cache-control": "no-cache",
    connection: "keep-alive",
  });
  for (const event of session.events) res.write(sseFrame(event));
  const listener = (event) => res.write(sseFrame(event));
  session.listeners.add(listener);
  req.on("close", () => session.listeners.delete(listener));
});

app.post("/v1/sessions/:id/abort", async (req, res) => {
  const session = sessions.get(req.params.id);
  if (!session) return res.status(404).json({ error: { message: "session not found" } });
  await abortRun({ sessionId: session.id, remoteState: session.remoteState }).catch(() => {});
  if (session.status === "running") {
    appendEvent(session, "session.error", { error: { message: "aborted" } });
    session.status = "error";
  }
  res.json({ aborted: true });
});

app.listen(PORT, () => {
  console.log(`[external-adapter] listening on :${PORT}`);
});
