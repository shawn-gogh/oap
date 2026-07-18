// session-pool.mjs — one independent `opencode serve` process per platform
// session, each with its own port and its own project directory FUSE-mounted
// from that session's MinIO workspace bucket.
//
// opencode's own session/message history lives in a SQLite db under
// $HOME/.local/share/opencode (confirmed empirically — NOT under the project
// cwd), so giving each session process an isolated, persisted HOME directory
// is what makes history survive an idle-eviction + later respawn: the FUSE
// mount only carries the user's workspace files, not opencode's own state.
import { spawn, execFile } from "node:child_process";
import { mkdir, writeFile, chmod, rm } from "node:fs/promises";
import { createServer } from "node:net";
import path from "node:path";
import { promisify } from "node:util";

import {
  startOpencode,
  provisionAgent,
  writeMcpConfig,
  writeProviderConfig,
  ensureProviderModel,
  gitInit,
} from "./opencode.mjs";

const execFileP = promisify(execFile);

const SESSIONS_ROOT = process.env.SESSIONS_ROOT || "/data/opencode-sessions";
const IDLE_TIMEOUT_MS = Number(process.env.SESSION_IDLE_TIMEOUT_MS || 15 * 60_000);
const MAX_CONCURRENT_SESSIONS = Number(process.env.SESSION_MAX_CONCURRENT || 20);
const SWEEP_INTERVAL_MS = 60_000;

// sessionId -> { proc, baseUrl, port, homeDir, mountDir, bucket, lastActivity }
const pool = new Map();

function getFreePort() {
  return new Promise((resolve, reject) => {
    const srv = createServer();
    srv.unref();
    srv.on("error", reject);
    srv.listen(0, "127.0.0.1", () => {
      const { port } = srv.address();
      srv.close(() => resolve(port));
    });
  });
}

async function writePasswdFile(sessionId, accessKey, secretKey) {
  const file = path.join(SESSIONS_ROOT, sessionId, ".s3fs-passwd");
  await writeFile(file, `${accessKey}:${secretKey}\n`, "utf8");
  // s3fs refuses to start unless the passwd file is exactly owner-readable.
  await chmod(file, 0o600);
  return file;
}

async function mountBucket({ sessionId, bucket, mountDir, minioEndpoint, accessKey, secretKey }) {
  await mkdir(mountDir, { recursive: true });
  const passwdFile = await writePasswdFile(sessionId, accessKey, secretKey);
  await execFileP("s3fs", [
    bucket,
    mountDir,
    "-o", `url=${minioEndpoint}`,
    "-o", "use_path_request_style",
    "-o", `passwd_file=${passwdFile}`,
    "-o", "nonempty",
  ]);
}

// Builds an authenticated CONNECT-proxy URL for this session: username is
// the session id (how lap's egress_proxy looks up the session's
// approval_mode), password is the same gateway key already used for every
// other call this wrapper makes to lap — no new secret to manage.
function proxyURLFor(sessionId, litellmApiKey, egressProxyURL) {
  const url = new URL(egressProxyURL);
  url.username = encodeURIComponent(sessionId);
  url.password = encodeURIComponent(litellmApiKey || "");
  return url.toString();
}

async function unmountBucket(mountDir) {
  try {
    await execFileP("fusermount", ["-u", mountDir]);
  } catch (err) {
    console.warn(`[workspace] unmount ${mountDir} failed (already unmounted?):`, err?.message || err);
  }
}

// Copies the currently-stored agent config into a freshly (re)mounted
// session's project directory — opencode requires .opencode/agent/*.md and
// opencode.json to live in its own cwd, so each per-session process needs
// its own copy rather than sharing the old global WORKDIR's files.
async function provisionInto(mountDir, { store, agentId, defaultModelProviderID, litellmProviderID }) {
  await gitInit(mountDir);
  const agents = store.listAgents();
  const row = agentId ? store.getAgent(agentId) : null;
  if (row) {
    await provisionAgent(mountDir, row);
  }
  await writeMcpConfig(mountDir, agents);
  return row;
}

/**
 * Returns the pool entry for `sessionId`, creating (mounting + spawning) it
 * if necessary. `deps` carries the collaborators session-pool doesn't own
 * (agent store, litellm provider config) so this stays testable/injectable
 * like the rest of this wrapper.
 */
export async function ensureSessionProcess(sessionId, opts) {
  const existing = pool.get(sessionId);
  if (existing) {
    existing.lastActivity = Date.now();
    return existing;
  }
  if (pool.size >= MAX_CONCURRENT_SESSIONS) {
    throw new Error(
      `workspace session limit reached (${MAX_CONCURRENT_SESSIONS} concurrent) — try again shortly`
    );
  }

  const {
    bucket,
    minioEndpoint,
    minioAccessKey,
    minioSecretKey,
    agentId,
    store,
    defaultModelProviderID,
    litellmProviderID,
    litellmModel,
    egressProxyURL,
    litellmApiKey,
  } = opts;

  const sessionDir = path.join(SESSIONS_ROOT, sessionId);
  const homeDir = path.join(sessionDir, "home");
  const mountDir = path.join(sessionDir, "workspace");
  await mkdir(homeDir, { recursive: true });

  await mountBucket({
    sessionId,
    bucket,
    mountDir,
    minioEndpoint,
    accessKey: minioAccessKey,
    secretKey: minioSecretKey,
  });

  try {
    await provisionInto(mountDir, { store, agentId, defaultModelProviderID, litellmProviderID });
    if (litellmModel?.baseURL && litellmModel?.apiKey) {
      await writeProviderConfig(mountDir, { ...litellmModel, sessionId });
    }
    const row = agentId ? store.getAgent(agentId) : null;
    if (row?.model && litellmModel?.providerID) {
      await ensureProviderModel(mountDir, { providerID: litellmModel.providerID, modelID: row.model });
    }

    const port = await getFreePort();
    // Force this session's outbound HTTP(S) through lap's egress proxy so
    // network access is enforced (approval_mode + domain whitelist) even for
    // tools like bash-invoked curl that never self-report a permission ask.
    // NO_PROXY must cover every internal service name/loopback this process
    // itself talks to — lap (LLM calls, platform MCP, tool-approvals bridge)
    // and minio — or those calls get routed into the proxy too and the
    // session breaks outright.
    const proxyEnv = egressProxyURL
      ? {
          HTTP_PROXY: proxyURLFor(sessionId, litellmApiKey, egressProxyURL),
          HTTPS_PROXY: proxyURLFor(sessionId, litellmApiKey, egressProxyURL),
          NO_PROXY: "localhost,127.0.0.1,lap,minio,postgres",
        }
      : {};
    const { baseUrl, proc, stop } = await startOpencode({
      port,
      cwd: mountDir,
      env: { HOME: homeDir, ...proxyEnv },
    });

    const entry = {
      proc,
      stop,
      baseUrl,
      port,
      homeDir,
      mountDir,
      bucket,
      lastActivity: Date.now(),
    };
    pool.set(sessionId, entry);
    proc.on("exit", () => {
      // Process died on its own (crash, OOM) — drop it from the pool so the
      // next request respawns cleanly instead of routing to a dead baseUrl.
      if (pool.get(sessionId) === entry) pool.delete(sessionId);
    });
    return entry;
  } catch (err) {
    await unmountBucket(mountDir);
    throw err;
  }
}

export function getSessionProcess(sessionId) {
  const entry = pool.get(sessionId);
  if (entry) entry.lastActivity = Date.now();
  return entry;
}

export function hasSessionProcess(sessionId) {
  return pool.has(sessionId);
}

export async function releaseSessionProcess(sessionId, { deleteFiles = false } = {}) {
  const entry = pool.get(sessionId);
  pool.delete(sessionId);
  if (entry) {
    try {
      entry.stop();
    } catch {
      /* ignore */
    }
    await unmountBucket(entry.mountDir);
  }
  if (deleteFiles) {
    await rm(path.join(SESSIONS_ROOT, sessionId), { recursive: true, force: true }).catch(() => {});
  }
}

let sweepTimer = null;
export function startIdleSweep() {
  if (sweepTimer) return;
  sweepTimer = setInterval(() => {
    const now = Date.now();
    for (const [sessionId, entry] of pool) {
      if (now - entry.lastActivity > IDLE_TIMEOUT_MS) {
        console.log(`[workspace] evicting idle session ${sessionId} (idle ${Math.round((now - entry.lastActivity) / 1000)}s)`);
        releaseSessionProcess(sessionId).catch((err) =>
          console.error(`[workspace] failed to evict ${sessionId}:`, err?.message || err)
        );
      }
    }
  }, SWEEP_INTERVAL_MS);
  sweepTimer.unref?.();
}

export async function stopAllSessionProcesses() {
  const ids = Array.from(pool.keys());
  await Promise.all(ids.map((id) => releaseSessionProcess(id)));
}
