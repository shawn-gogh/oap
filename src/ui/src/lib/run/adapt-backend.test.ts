import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

import { adaptArtifactResponse, adaptSnapshot } from "./adapt-backend";
import type { BackendArtifactResponse, BackendRunSnapshotV1 } from "./backend-types";

// Ground-truthed against the real backend's own committed contract
// fixtures (tests/fixtures/run_contract/*.json, from codex/run-control-plane)
// rather than hand-written mocks, so a drift in the real API shape shows up
// here rather than only in production.
const FIXTURE_DIR = fileURLToPath(new URL("../../../../../tests/fixtures/run_contract/", import.meta.url));

function loadFixture<T>(name: string): T {
  return JSON.parse(readFileSync(`${FIXTURE_DIR}${name}`, "utf8")) as T;
}

describe("adaptSnapshot", () => {
  it("adapts a completed run", () => {
    const backend = loadFixture<BackendRunSnapshotV1>("run-snapshot-completed-v1.json");
    const adapted = adaptSnapshot(backend);
    expect(adapted.runId).toBe("turn_completed");
    expect(adapted.sessionId).toBe("session_example");
    expect(adapted.status).toBe("completed");
    expect(adapted.interactionProfile.supportsCancel).toBe(false);
    expect(adapted.interactionProfile.supportsRetry).toBe(true);
    expect(adapted.result).toEqual({ kind: "json", json: { summary: "Assessment complete" } });
    expect(adapted.inputSnapshot).toEqual({ topic: "agent interoperability" });
    expect(adapted.lastEventSeq).toBe(12);
  });

  it("adapts a running turn with the fuller SessionTurnRow present", () => {
    const backend = loadFixture<BackendRunSnapshotV1>("run-snapshot-running-v1.json");
    const adapted = adaptSnapshot(backend);
    expect(adapted.status).toBe("running");
    expect(adapted.interactionProfile.supportsCancel).toBe(true);
    expect(adapted.trigger).toBe("user"); // trigger_type "manual" maps to "user"
    expect(adapted.result).toBeNull();
    expect(adapted.pendingApproval).toBeNull();
    expect(adapted.pendingInputRequest).toBeNull();
  });

  it("adapts a waiting_input turn's pending request into the known-gap generic text field", () => {
    const backend = loadFixture<BackendRunSnapshotV1>("run-snapshot-waiting-input-v1.json");
    const adapted = adaptSnapshot(backend);
    expect(adapted.status).toBe("waiting_input");
    expect(adapted.pendingApproval).toBeNull();
    expect(adapted.pendingInputRequest).not.toBeNull();
    expect(adapted.pendingInputRequest?.id).toBe("input_request_example");
    expect(adapted.pendingInputRequest?.prompt).toBe("Select a region");
    expect(adapted.pendingInputRequest?.fields).toEqual([
      { id: "input", label: "Select a region", kind: "text", required: true },
    ]);
  });

  it("tolerates a SessionTurnRow trimmed down to its required fields only", () => {
    // run-snapshot-completed-v1.json's `turn` object omits input_json,
    // interaction_profile_json, timestamps, etc. — exactly the case
    // backend-types.ts's optional fields exist to guard against.
    const backend = loadFixture<BackendRunSnapshotV1>("run-snapshot-completed-v1.json");
    expect(() => adaptSnapshot(backend)).not.toThrow();
  });

  it("infers a required progress-capable field set without throwing on a minimal interaction profile", () => {
    const backend = loadFixture<BackendRunSnapshotV1>("run-snapshot-waiting-input-v1.json");
    const adapted = adaptSnapshot(backend);
    expect(adapted.interactionProfile.inputSchema).toEqual({ type: "object" });
  });
});

describe("adaptArtifactResponse", () => {
  it("prefers download_url, falling back to external_reference_url", () => {
    const base: BackendArtifactResponse = {
      id: "artifact_1",
      session_id: "ses_1",
      turn_id: "turn_1",
      invocation_id: null,
      task_id: null,
      source_artifact_id: "report",
      media_type: "application/json",
      digest: null,
      size_bytes: 128,
      status: "verified",
      metadata: { name: "report.json" },
      created_at: 0,
      verified_at: 0,
      download_url: "https://example.com/download",
      external_reference_url: null,
    };
    expect(adaptArtifactResponse(base).url).toBe("https://example.com/download");
    expect(adaptArtifactResponse(base).name).toBe("report.json");

    const externalOnly = { ...base, download_url: null, external_reference_url: "https://example.com/ext" };
    expect(adaptArtifactResponse(externalOnly).url).toBe("https://example.com/ext");
  });

  it("falls back to source_artifact_id when metadata has no name", () => {
    const row: BackendArtifactResponse = {
      id: "artifact_2",
      session_id: "ses_1",
      turn_id: "turn_1",
      invocation_id: null,
      task_id: null,
      source_artifact_id: "trace",
      media_type: "application/json",
      digest: null,
      size_bytes: null,
      status: "verified",
      metadata: {},
      created_at: 0,
      verified_at: null,
      download_url: null,
      external_reference_url: null,
    };
    expect(adaptArtifactResponse(row).name).toBe("trace");
    expect(adaptArtifactResponse(row).url).toBeNull();
  });
});
