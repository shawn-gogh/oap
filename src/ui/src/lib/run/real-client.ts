// Real transport: implements RunTransport against the live backend built by
// codex/run-control-plane (now merged into main). See the Stage 7 plan's
// "Design decisions" for why this exists alongside fixture-client.ts rather
// than replacing it. Event handling (subscribeRunEvents below) patches in
// place using adaptControlEvent's per-event-type translation (Stage 6),
// falling back to a full snapshot refetch only for the cases documented on
// `AdaptedControlEvent`/`ControlEventV1["snapshot.replaced"]`.

import {
  acceptApproval,
  cancelTurn,
  createRunTurn,
  getRunArtifact,
  getRunTurn,
  rejectApproval,
  resumeRunTurn,
  retryRunTurn,
  subscribeControlEvents,
} from "@/lib/api";
import { adaptArtifactResponse, adaptControlEvent, adaptSnapshot } from "./adapt-backend";
import type { BackendArtifactResponse, BackendControlEventV1, BackendRunSnapshotV1 } from "./backend-types";
import type {
  ControlEventV1,
  RunApprovalDecisionCommand,
  RunCancelCommand,
  RunCreateCommand,
  RunResumeCommand,
  RunRetryCommand,
  RunSnapshotV1,
} from "./types";
import type { RunTransport } from "./transport";

/** Fetches one artifact's presigned download URL and merges it into an
 * already-adapted snapshot's matching entry — the snapshot's own embedded
 * artifact rows never carry `download_url` (see adapt-backend.ts). Errors
 * are swallowed: a missing/failed download link degrades to no link, it
 * shouldn't fail the whole snapshot load. */
async function enrichArtifactUrls(sessionId: string, snapshot: RunSnapshotV1): Promise<RunSnapshotV1> {
  if (snapshot.artifacts.length === 0) return snapshot;
  const enriched = await Promise.all(
    snapshot.artifacts.map(async (artifact) => {
      try {
        const raw = (await getRunArtifact(sessionId, artifact.id)) as BackendArtifactResponse;
        return adaptArtifactResponse(raw);
      } catch {
        return artifact;
      }
    }),
  );
  return { ...snapshot, artifacts: enriched };
}

/** Binds a session id once so the returned object satisfies `RunTransport`
 * (whose methods only take a `runId` = turn id) even though every real
 * route is session-scoped (`/api/sessions/{sessionId}/turns/...`). */
export function createRealRunTransport(sessionId: string): RunTransport {
  const getRunSnapshot = async (runId: string): Promise<RunSnapshotV1> => {
    const raw = (await getRunTurn(sessionId, runId)) as BackendRunSnapshotV1;
    return enrichArtifactUrls(sessionId, adaptSnapshot(raw));
  };

  return {
    getRunSnapshot,

    subscribeRunEvents(runId, fromSeq, onEvent) {
      // Uses subscribeControlEvents (api.ts), NOT the native EventSource —
      // every real frame is a *named* SSE event (`event: turn.accepted`,
      // ...), and EventSource.onmessage only fires for frames with no
      // `event:` field, so a plain EventSource silently receives nothing
      // from this endpoint (confirmed against the live backend). See that
      // function's doc comment for the full explanation.
      return subscribeControlEvents({
        sessionId,
        afterSequence: fromSeq,
        onFrame: (lastEventId, data) => {
          const event = data as BackendControlEventV1;
          const sequence = event.sequence ?? (lastEventId ? Number(lastEventId) : NaN);
          const adapted = adaptControlEvent(event);

          if (adapted === null) return; // not worth patching or reloading over

          if (adapted !== "refetch") {
            onEvent({ seq: sequence, ts: event.occurred_at, ...adapted });
            return;
          }

          // The one genuinely-unpatchable case (a brand-new invocation, or
          // an unrecognized event_type) — fall back to one authoritative
          // refetch, delivered as a wholesale snapshot replacement.
          void getRunSnapshot(runId)
            .then((snapshot) => {
              onEvent({
                seq: Number.isFinite(sequence) ? sequence : snapshot.lastEventSeq,
                ts: Date.now(),
                type: "snapshot.replaced",
                snapshot,
              });
            })
            .catch(() => {
              // A transient refetch failure just means this frame is
              // skipped — the next frame (or a manual reload) will catch up.
            });
        },
      });
    },

    async submitRunInput(cmd: RunResumeCommand): Promise<RunSnapshotV1> {
      const raw = (await resumeRunTurn(sessionId, cmd.runId, {
        request_id: cmd.requestId,
        // The known-gap generic text field (adapt-backend.ts) is always
        // keyed "input" — see RunInputForm's submission path.
        input: { input: cmd.values.input ?? Object.values(cmd.values)[0] ?? "" },
      })) as BackendRunSnapshotV1;
      return enrichArtifactUrls(sessionId, adaptSnapshot(raw));
    },

    async decideRunApproval(cmd: RunApprovalDecisionCommand): Promise<RunSnapshotV1> {
      // Approvals are decided through the inbox/approvals API, not a
      // turn-scoped route (src/http/managed_agents/inbox/approvals.rs) —
      // its response doesn't carry a snapshot, so follow up with a fetch.
      if (cmd.decision === "accepted") {
        await acceptApproval(cmd.approvalId);
      } else {
        await rejectApproval(cmd.approvalId, cmd.feedback);
      }
      return getRunSnapshot(cmd.runId);
    },

    async cancelRun(cmd: RunCancelCommand): Promise<RunSnapshotV1> {
      // cancel_turn returns the smaller {turn, invocations} shape, not a
      // full RunSnapshotV1 (the asymmetric-response finding) — discard it
      // and re-fetch so every RunTransport method returns the same shape.
      await cancelTurn(sessionId, cmd.runId);
      return getRunSnapshot(cmd.runId);
    },

    async retryRun(cmd: RunRetryCommand): Promise<RunSnapshotV1> {
      // Retrying creates a NEW turn (turn.retry_of_turn_id points back at
      // this one) — the returned snapshot's runId differs from cmd.runId.
      const raw = (await retryRunTurn(sessionId, cmd.runId)) as BackendRunSnapshotV1;
      return enrichArtifactUrls(sessionId, adaptSnapshot(raw));
    },

    async createRun(cmd: RunCreateCommand): Promise<RunSnapshotV1> {
      if (!cmd.sessionId) {
        throw new Error("createRun requires sessionId for the real transport.");
      }
      const raw = (await createRunTurn(cmd.sessionId, { input: cmd.input })) as BackendRunSnapshotV1;
      return enrichArtifactUrls(cmd.sessionId, adaptSnapshot(raw));
    },
  };
}
