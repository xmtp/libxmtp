import assert from "node:assert/strict";
import { test } from "node:test";

import { SessionCopy, SessionError } from "./sessionError.ts";
import { type SettingsPorts, SettingsModel } from "./settingsModel.ts";

function ports(overrides: Partial<SettingsPorts> = {}): SettingsPorts & {
  calls: string[];
} {
  const calls: string[] = [];
  return {
    calls,
    signOut: () => {
      calls.push("signOut");
      return Promise.resolve();
    },
    deleteLocalData: () => {
      calls.push("delete");
      return Promise.resolve();
    },
    continueReset: () => {
      calls.push("continue");
      return Promise.resolve();
    },
    setAppearance: () => Promise.resolve(),
    ...overrides,
  };
}

test("one device action runs at a time", async () => {
  let release = () => {};
  const fake = ports({
    signOut: () =>
      new Promise<void>((resolve) => {
        release = resolve;
      }),
  });
  const model = new SettingsModel(fake);
  const first = model.signOut();
  assert.equal(model.getState().busy, true);
  await model.deleteLocalData();
  assert.deepEqual(fake.calls, [], "ignored while busy");
  release();
  await first;
  assert.equal(model.getState().busy, false);
  assert.equal(model.getState().error, undefined);
});

test("each failed device action shows its error", async () => {
  const failing = () => Promise.reject(new Error("disk"));
  const model = new SettingsModel(
    ports({
      signOut: failing,
      deleteLocalData: failing,
      continueReset: failing,
    }),
  );
  // Each failure replaces the error the one before it showed.
  await model.signOut();
  assert.equal(model.getState().error, SessionCopy.signOutFailed);
  await model.continueReset();
  assert.equal(model.getState().error, SessionCopy.resetIncomplete);
  await model.signOut();
  await model.deleteLocalData();
  assert.equal(model.getState().error, SessionCopy.resetIncomplete);
  assert.equal(model.getState().busy, false);

  const appearance = new SettingsModel(
    ports({
      setAppearance: () =>
        Promise.reject(SessionError.message(SessionCopy.appearanceFailed)),
    }),
  );
  await appearance.setAppearance("dark");
  assert.equal(appearance.getState().error, SessionCopy.appearanceFailed);
});

test("results after the screen closes are dropped, and the reopened screen takes the next action", async () => {
  const model = new SettingsModel(
    ports({ signOut: () => Promise.reject(new Error("late")) }),
  );
  const running = model.signOut();
  model.dispose();
  await running;
  assert.equal(model.getState().error, undefined);
  // A React effect runs again after its cleanup, with the same model.
  model.appear();
  await model.signOut();
  assert.equal(model.getState().error, SessionCopy.signOutFailed);
});
