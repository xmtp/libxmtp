import { describe, it, expect } from "vitest";
import { evaluateGate } from "../src/lib/cross-test-gate";

// Shape mirrors `gh api .../workflows/cross-test.yml/runs?head_sha=SHA&status=completed`
const run = (over: Partial<Record<string, unknown>> = {}) => ({
  head_sha: "abc1234def",
  status: "completed",
  conclusion: "success",
  ...over,
});

describe("evaluateGate", () => {
  const SHA = "abc1234def";

  it("passes when a completed successful run exists for the exact SHA", () => {
    const res = evaluateGate({ workflow_runs: [run()] }, SHA);
    expect(res.pass).toBe(true);
  });

  it("fails (skip) when no run matches the SHA", () => {
    const res = evaluateGate(
      { workflow_runs: [run({ head_sha: "other" })] },
      SHA,
    );
    expect(res.pass).toBe(false);
    expect(res.reason).toMatch(/no .*run.*SHA/i);
  });

  it("fails (skip) when the matching run did not succeed", () => {
    const res = evaluateGate(
      { workflow_runs: [run({ conclusion: "failure" })] },
      SHA,
    );
    expect(res.pass).toBe(false);
    expect(res.reason).toMatch(/not success|failure/i);
  });

  it("fails (skip) when the matching run is not completed", () => {
    const res = evaluateGate(
      { workflow_runs: [run({ status: "in_progress", conclusion: null })] },
      SHA,
    );
    expect(res.pass).toBe(false);
  });

  it("fails (skip) on an empty run list", () => {
    const res = evaluateGate({ workflow_runs: [] }, SHA);
    expect(res.pass).toBe(false);
  });

  it("passes when any green run for the SHA exists despite a later failed re-run", () => {
    const res = evaluateGate(
      {
        workflow_runs: [
          run({ conclusion: "failure" }),
          run({ conclusion: "success" }),
        ],
      },
      SHA,
    );
    expect(res.pass).toBe(true);
  });

  it("does not pass on a 'skipped' conclusion (only 'success' counts)", () => {
    const res = evaluateGate(
      { workflow_runs: [run({ conclusion: "skipped" })] },
      SHA,
    );
    expect(res.pass).toBe(false);
  });

  it("does not let a green run on a DIFFERENT sha unblock this sha", () => {
    const res = evaluateGate(
      {
        workflow_runs: [
          run({ head_sha: "othersha", conclusion: "success" }),
          run({ conclusion: "failure" }),
        ],
      },
      SHA,
    );
    expect(res.pass).toBe(false);
  });

  it("tolerates a missing workflow_runs key (fail-closed)", () => {
    const res = evaluateGate({} as { workflow_runs: never[] }, SHA);
    expect(res.pass).toBe(false);
  });
});
