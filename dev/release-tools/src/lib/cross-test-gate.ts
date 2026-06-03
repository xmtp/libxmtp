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
 * Semantics, made explicit:
 * - SHA-exact: only runs whose `head_sha` equals the release SHA count, so a
 *   green run on *different* code can never unblock this commit (no stale-green
 *   across content).
 * - "any green for this SHA", not "latest run only": a later re-run that is
 *   pending/failed/`skipped` does NOT retract an earlier genuine success for the
 *   *same* commit — the code was proven green at least once. Conversely a single
 *   `success` is required; `conclusion` values other than "success" (incl.
 *   `null`, `failure`, `cancelled`, `skipped`, `timed_out`) never pass.
 * - Caller passes only `status=completed` runs from the GitHub API, but we
 *   re-filter on `status === "completed"` defensively in case that changes.
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
