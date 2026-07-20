import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

import { adaptArtifactResponse, adaptControlEvent, adaptSnapshot } from "./adapt-backend";
import type { BackendArtifactResponse, BackendControlEventV1, BackendRunSnapshotV1 } from "./backend-types";

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

// The base envelope every backend control event carries — individual tests
// override `type`/`payload`/`invocation_id` as needed. Matches
// src/http/sessions/run_types.rs's ControlEventV1 exactly.
function baseEvent(overrides: Partial<BackendControlEventV1>): BackendControlEventV1 {
  return {
    schema_version: 1,
    type: "turn.updated",
    sequence: 1,
    session_id: "ses_1",
    turn_id: "turn_1",
    invocation_id: null,
    request_id: "req_1",
    occurred_at: 1000,
    payload: {},
    ...overrides,
  };
}

describe("adaptControlEvent", () => {
  it("adapts any turn.* event via its payload's own status/error, not the event_type suffix", () => {
    const event = baseEvent({ type: "turn.failed", payload: { status: "failed", error: { code: "x", message: "boom", retryable: false } } });
    expect(adaptControlEvent(event)).toEqual({
      type: "turn.status_changed",
      status: "failed",
      error: { code: "x", message: "boom", retryable: false },
    });
  });

  it("adapts a turn.* event with a null error", () => {
    const event = baseEvent({ type: "turn.completed", payload: { status: "completed", error: null } });
    expect(adaptControlEvent(event)).toEqual({ type: "turn.status_changed", status: "completed", error: null });
  });

  it("always refetches for invocation.accepted (payload can't rebuild a full row)", () => {
    const event = baseEvent({
      type: "invocation.accepted",
      invocation_id: "inv_1",
      payload: { status: "queued", parent_invocation_id: null, role: "tool" },
    });
    expect(adaptControlEvent(event)).toBe("refetch");
  });

  it("patches an invocation status in place for any other invocation.* event", () => {
    const event = baseEvent({
      type: "invocation.completed",
      invocation_id: "inv_1",
      payload: { status: "completed", error: null },
    });
    expect(adaptControlEvent(event)).toEqual({
      type: "invocation.status_changed",
      invocationId: "inv_1",
      status: "completed",
    });
  });

  it("refetches an invocation.* event missing invocation_id rather than guessing", () => {
    const event = baseEvent({ type: "invocation.completed", invocation_id: null, payload: { status: "completed" } });
    expect(adaptControlEvent(event)).toBe("refetch");
  });

  it("adapts artifact.added directly from the event's embedded full row", () => {
    const event = baseEvent({
      type: "artifact.added",
      payload: {
        schema_version: 1,
        artifact: {
          id: "artifact_1",
          session_id: "ses_1",
          turn_id: "turn_1",
          invocation_id: null,
          task_id: null,
          source_artifact_id: "report",
          media_type: "text/markdown",
          digest: null,
          size_bytes: 10,
          status: "verified",
          metadata: { name: "report.md" },
          created_at: 0,
          verified_at: 0,
        },
      },
    });
    const adapted = adaptControlEvent(event);
    expect(adapted).toMatchObject({ type: "artifact.added", artifact: { id: "artifact_1", name: "report.md", mediaType: "text/markdown" } });
  });

  it("adapts input.requested into the known-gap generic field when payload.fields isn't recognizably structured", () => {
    const event = baseEvent({
      type: "input.requested",
      payload: { request_id: "req_x", prompt: "Pick a region", schema: null, fields: null },
    });
    expect(adaptControlEvent(event)).toEqual({
      type: "input_request.created",
      request: {
        id: "req_x",
        requestedAt: 1000,
        prompt: "Pick a region",
        fields: [{ id: "input", label: "Pick a region", kind: "text", required: true }],
      },
    });
  });

  it("adapts input.requested using real structured fields when payload.fields matches RunInputRequestField's shape", () => {
    const event = baseEvent({
      type: "input.requested",
      payload: {
        request_id: "req_y",
        prompt: "More detail needed",
        fields: [{ id: "count", label: "Count", kind: "text", required: true }],
      },
    });
    expect(adaptControlEvent(event)).toEqual({
      type: "input_request.created",
      request: {
        id: "req_y",
        requestedAt: 1000,
        prompt: "More detail needed",
        fields: [{ id: "count", label: "Count", kind: "text", required: true }],
      },
    });
  });

  it("adapts approval.requested directly from the event's embedded approval object", () => {
    const event = baseEvent({
      type: "approval.requested",
      payload: {
        approval: { id: "appr_1", kind: "runtime_permission", title: "Allow tool?", body: "details", created_at: 999 },
      },
    });
    expect(adaptControlEvent(event)).toEqual({
      type: "approval.created",
      approval: { id: "appr_1", kind: "runtime_permission", title: "Allow tool?", body: "details", requestedAt: 999, canDecide: true },
    });
  });

  it("adapts approval.resolved", () => {
    const event = baseEvent({
      type: "approval.resolved",
      payload: { approval_id: "appr_1", decision: "rejected", feedback: "no thanks" },
    });
    expect(adaptControlEvent(event)).toEqual({
      type: "approval.resolved",
      approvalId: "appr_1",
      decision: "rejected",
      feedback: "no thanks",
    });
  });

  it("adapts result.completed into the existing turn.result variant", () => {
    const event = baseEvent({ type: "result.completed", payload: { result: { summary: "done" } } });
    expect(adaptControlEvent(event)).toEqual({
      type: "turn.result",
      result: { kind: "json", json: { summary: "done" } },
    });
  });

  it("adapts message.completed into message.appended when a text part and invocation_id are present", () => {
    const event = baseEvent({
      type: "message.completed",
      invocation_id: "inv_1",
      payload: { message: { content: [{ type: "text", text: "hello" }] } },
    });
    expect(adaptControlEvent(event)).toEqual({ type: "message.appended", invocationId: "inv_1", text: "hello" });
  });

  it("ignores an unparseable message.completed rather than refetching over it", () => {
    const event = baseEvent({ type: "message.completed", invocation_id: "inv_1", payload: { message: {} } });
    expect(adaptControlEvent(event)).toBeNull();

    const missingInvocation = baseEvent({
      type: "message.completed",
      invocation_id: null,
      payload: { message: { content: [{ type: "text", text: "hello" }] } },
    });
    expect(adaptControlEvent(missingInvocation)).toBeNull();
  });

  it("refetches for any unrecognized event_type as a defensive fallback", () => {
    const event = baseEvent({ type: "some_future_event_type", payload: {} });
    expect(adaptControlEvent(event)).toBe("refetch");
  });
});
