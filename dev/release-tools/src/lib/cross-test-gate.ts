export interface CrossTestRun {
  head_sha: string;
  status: string;
  conclusion: string | null;
}

export interface CrossTestRunsPayload {
  workflow_runs: CrossTestRun[];
}

export interface GateResult {
  pass: boolean;
  reason: string;
}

/**
 * Decide whether the nightly may ship: there must be a cross-test run for the
 * EXACT release SHA that is completed AND concluded success. Fail-closed —
 * any ambiguity (no run, wrong SHA, not completed, not success) skips.
 *
 * Two non-obvious decisions:
 * - SHA-exact: a green run on *different* code can't unblock this commit.
 * - "any green for this SHA", not "latest run only": a later failed/`skipped`
 *   re-run doesn't retract an earlier genuine success for the same commit.
 */
export function evaluateGate(
  payload: CrossTestRunsPayload,
  sha: string,
): GateResult {
  const forSha = (payload.workflow_runs ?? []).filter(
    (r) => r.head_sha === sha,
  );
  if (forSha.length === 0) {
    return { pass: false, reason: `No cross-test run found for SHA ${sha}` };
  }
  const completed = forSha.filter((r) => r.status === "completed");
  if (completed.length === 0) {
    return {
      pass: false,
      reason: `cross-test run for ${sha} is not completed`,
    };
  }
  const success = completed.some((r) => r.conclusion === "success");
  if (!success) {
    return {
      pass: false,
      reason: `cross-test for ${sha} did not conclude success (not success)`,
    };
  }
  return { pass: true, reason: `cross-test green for ${sha}` };
}
