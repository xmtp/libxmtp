import assert from "node:assert/strict";
import { test } from "node:test";

import {
  type ResetJournal,
  type ResetPhase,
  continueResetJournal,
  isResetPhase,
} from "./resetJournal.ts";

function recorder() {
  const steps: string[] = [];
  return {
    steps,
    ops: {
      removeDatabase: (id: string) => {
        steps.push(`database ${id}`);
        return Promise.resolve();
      },
      removeProfileFiles: (id: string) => {
        steps.push(`files ${id}`);
        return Promise.resolve();
      },
      removeSecrets: (id: string) => {
        steps.push(`secrets ${id}`);
        return Promise.resolve();
      },
      saveJournal: (journal: ResetJournal) => {
        steps.push(`save ${journal.phase}`);
        return Promise.resolve();
      },
      finish: (id: string) => {
        steps.push(`finish ${id}`);
        return Promise.resolve();
      },
    },
  };
}

test("resumes from each phase and saves each phase before the next step", async () => {
  const expected: Record<ResetPhase, string[]> = {
    STOPPING: [
      "database p",
      "save DATABASE_REMOVED",
      "files p",
      "save FILES_REMOVED",
      "secrets p",
      "finish p",
    ],
    DATABASE_REMOVED: [
      "files p",
      "save FILES_REMOVED",
      "secrets p",
      "finish p",
    ],
    FILES_REMOVED: ["secrets p", "finish p"],
  };
  for (const phase of [
    "STOPPING",
    "DATABASE_REMOVED",
    "FILES_REMOVED",
  ] as const) {
    const { steps, ops } = recorder();
    await continueResetJournal({ profileId: "p", phase }, ops);
    assert.deepEqual(steps, expected[phase], phase);
  }
});

test("phase names match the iOS raw values", () => {
  for (const phase of ["STOPPING", "DATABASE_REMOVED", "FILES_REMOVED"]) {
    assert.ok(isResetPhase(phase));
  }
  assert.ok(!isResetPhase("stopping"));
  assert.ok(!isResetPhase(undefined));
});
