// Shared interface both the fixture-backed transport (fixture-client.ts,
// Stage 1-3's dev-only demo data) and the real transport (real-client.ts,
// this stage) satisfy. RunShell/RunInputForm depend on this interface, not
// on either concrete implementation, so fixture-mode demos and real usage
// can share the same components without a fork.

import type {
  ControlEventV1,
  RunApprovalDecisionCommand,
  RunCancelCommand,
  RunCreateCommand,
  RunResumeCommand,
  RunRetryCommand,
  RunSnapshotV1,
} from "./types";

export interface RunTransport {
  getRunSnapshot(runId: string): Promise<RunSnapshotV1>;
  subscribeRunEvents(
    runId: string,
    fromSeq: number,
    onEvent: (event: ControlEventV1) => void,
  ): () => void;
  submitRunInput(cmd: RunResumeCommand): Promise<RunSnapshotV1>;
  decideRunApproval(cmd: RunApprovalDecisionCommand): Promise<RunSnapshotV1>;
  cancelRun(cmd: RunCancelCommand): Promise<RunSnapshotV1>;
  retryRun(cmd: RunRetryCommand): Promise<RunSnapshotV1>;
  createRun(cmd: RunCreateCommand): Promise<RunSnapshotV1>;
}
