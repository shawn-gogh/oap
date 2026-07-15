// store.mjs
//
// Durable SQLite-backed store for an opencode-compatible agent server.
// Persists agent definitions and session->agent bindings in two tables
// (`agents`, `session_bindings`) using better-sqlite3 with WAL journaling.
//
// JSON-typed columns (permissions, mcp_servers, workspace) are transparently
// serialized on write and parsed on read, so callers always receive plain JS
// objects/arrays. Exposes a small CRUD surface plus session binding helpers.
// ESM only; Node 20+.

import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import Database from "better-sqlite3";

function genId() {
  return "agt_" + crypto.randomBytes(12).toString("hex");
}

// Convert a raw DB row into a caller-facing row with parsed JSON fields.
function deserialize(row) {
  if (!row) return null;
  return {
    id: row.id,
    name: row.name,
    system: row.system,
    model: row.model,
    permissions: row.permissions ? JSON.parse(row.permissions) : {},
    mcp_servers: row.mcp_servers ? JSON.parse(row.mcp_servers) : [],
    workspace: row.workspace ? JSON.parse(row.workspace) : null,
    created_at: row.created_at,
    updated_at: row.updated_at,
  };
}

export function createStore(dbPath) {
  const dir = path.dirname(dbPath);
  if (dir) fs.mkdirSync(dir, { recursive: true });

  const db = new Database(dbPath);
  db.pragma("journal_mode = WAL");

  db.exec(`
    CREATE TABLE IF NOT EXISTS agents (
      id TEXT PRIMARY KEY,
      name TEXT,
      system TEXT,
      model TEXT,
      permissions TEXT,
      mcp_servers TEXT,
      workspace TEXT,
      created_at INTEGER,
      updated_at INTEGER
    );
    CREATE TABLE IF NOT EXISTS session_bindings (
      session_id TEXT PRIMARY KEY,
      agent_id TEXT,
      created_at INTEGER
    );
    CREATE TABLE IF NOT EXISTS session_events (
      seq        INTEGER PRIMARY KEY AUTOINCREMENT,
      session_id TEXT NOT NULL,
      event_id   TEXT,
      event_json TEXT NOT NULL,
      UNIQUE(session_id, event_id)
    );
    CREATE INDEX IF NOT EXISTS idx_se_session
      ON session_events(session_id, seq);
  `);

  // better-sqlite3 has no migration system; ADD COLUMN against a db file that
  // predates the workspace feature needs to be applied by hand and is a
  // no-op (caught) on a fresh db where the CREATE TABLE above already has it.
  for (const ddl of [
    `ALTER TABLE session_bindings ADD COLUMN workspace_session_id TEXT`,
    `ALTER TABLE session_bindings ADD COLUMN workspace_bucket TEXT`,
  ]) {
    try {
      db.exec(ddl);
    } catch (err) {
      if (!/duplicate column/i.test(err?.message || "")) throw err;
    }
  }

  const stmts = {
    insert: db.prepare(`
      INSERT INTO agents
        (id, name, system, model, permissions, mcp_servers, workspace, created_at, updated_at)
      VALUES
        (@id, @name, @system, @model, @permissions, @mcp_servers, @workspace, @created_at, @updated_at)
    `),
    get: db.prepare(`SELECT * FROM agents WHERE id = ?`),
    list: db.prepare(`SELECT * FROM agents ORDER BY created_at ASC`),
    delete: db.prepare(`DELETE FROM agents WHERE id = ?`),
    bind: db.prepare(`
      INSERT INTO session_bindings (session_id, agent_id, created_at)
      VALUES (@session_id, @agent_id, @created_at)
      ON CONFLICT(session_id) DO UPDATE SET
        agent_id = excluded.agent_id,
        created_at = excluded.created_at
    `),
    getBinding: db.prepare(`SELECT agent_id FROM session_bindings WHERE session_id = ?`),
    unbind: db.prepare(`DELETE FROM session_bindings WHERE session_id = ?`),
    setWorkspace: db.prepare(`
      UPDATE session_bindings
      SET workspace_session_id = @workspace_session_id, workspace_bucket = @workspace_bucket
      WHERE session_id = @session_id
    `),
    getWorkspace: db.prepare(`
      SELECT workspace_session_id, workspace_bucket
      FROM session_bindings
      WHERE session_id = ?
    `),
  };

  function createAgent({ name, system, model, permissions, mcp_servers, workspace } = {}) {
    const now = Date.now();
    const row = {
      id: genId(),
      name: name ?? null,
      system: system ?? null,
      model: model ?? null,
      permissions: JSON.stringify(permissions ?? {}),
      mcp_servers: JSON.stringify(mcp_servers ?? []),
      workspace: workspace === undefined ? null : JSON.stringify(workspace),
      created_at: now,
      updated_at: now,
    };
    stmts.insert.run(row);
    return deserialize(stmts.get.get(row.id));
  }

  function getAgent(id) {
    return deserialize(stmts.get.get(id));
  }

  function listAgents() {
    return stmts.list.all().map(deserialize);
  }

  function updateAgent(id, patch = {}) {
    const existing = stmts.get.get(id);
    if (!existing) return null;

    const merged = { ...existing };
    if ("name" in patch) merged.name = patch.name ?? null;
    if ("system" in patch) merged.system = patch.system ?? null;
    if ("model" in patch) merged.model = patch.model ?? null;
    if ("permissions" in patch) merged.permissions = JSON.stringify(patch.permissions ?? {});
    if ("mcp_servers" in patch) merged.mcp_servers = JSON.stringify(patch.mcp_servers ?? []);
    if ("workspace" in patch)
      merged.workspace = patch.workspace === undefined || patch.workspace === null
        ? null
        : JSON.stringify(patch.workspace);
    merged.updated_at = Date.now();

    db.prepare(`
      UPDATE agents SET
        name = @name, system = @system, model = @model,
        permissions = @permissions, mcp_servers = @mcp_servers,
        workspace = @workspace, updated_at = @updated_at
      WHERE id = @id
    `).run(merged);

    return deserialize(stmts.get.get(id));
  }

  function deleteAgent(id) {
    return stmts.delete.run(id).changes > 0;
  }

  function bindSession(sessionId, agentId) {
    stmts.bind.run({ session_id: sessionId, agent_id: agentId, created_at: Date.now() });
  }

  function getSessionAgent(sessionId) {
    const row = stmts.getBinding.get(sessionId);
    return row ? row.agent_id : null;
  }

  function unbindSession(sessionId) {
    stmts.unbind.run(sessionId);
  }

  // Records which platform (LAP) session id + workspace bucket this
  // opencode-assigned session id belongs to, so a later request bearing only
  // the opencode session id (the id this wrapper's HTTP surface is keyed by)
  // can find its way back to the right per-session opencode process — even
  // after this wrapper container restarts and the in-memory pool is empty.
  function setSessionWorkspace(sessionId, { workspaceSessionId, workspaceBucket }) {
    stmts.setWorkspace.run({
      session_id: sessionId,
      workspace_session_id: workspaceSessionId ?? null,
      workspace_bucket: workspaceBucket ?? null,
    });
  }

  function getSessionWorkspace(sessionId) {
    const row = stmts.getWorkspace.get(sessionId);
    if (!row || !row.workspace_session_id) return null;
    return { workspaceSessionId: row.workspace_session_id, workspaceBucket: row.workspace_bucket };
  }

  function insertSessionEvent(sessionId, eventObj, eventId) {
    const json = JSON.stringify(eventObj);
    if (eventId != null) {
      db.prepare(`
        INSERT INTO session_events (session_id, event_id, event_json)
        VALUES (?, ?, ?)
        ON CONFLICT(session_id, event_id) DO UPDATE SET
          event_json = excluded.event_json
      `).run(sessionId, eventId, json);
    } else {
      db.prepare(`
        INSERT INTO session_events (session_id, event_json)
        VALUES (?, ?)
      `).run(sessionId, json);
    }
  }

  function listSessionEvents(sessionId) {
    return db
      .prepare(`SELECT seq, event_json FROM session_events WHERE session_id = ? ORDER BY seq ASC`)
      .all(sessionId)
      .map((r) => ({ seq: r.seq, ...JSON.parse(r.event_json) }));
  }

  return {
    createAgent,
    getAgent,
    listAgents,
    updateAgent,
    deleteAgent,
    bindSession,
    getSessionAgent,
    unbindSession,
    setSessionWorkspace,
    getSessionWorkspace,
    insertSessionEvent,
    listSessionEvents,
  };
}
