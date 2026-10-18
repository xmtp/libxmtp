import assert from "node:assert/strict";
import { test } from "node:test";

import {
  type HandleAccount,
  type HandleModelOptions,
  HandleFailure,
  HandleModel,
  canRetryNotice,
} from "./handleModel.ts";
import { type HandleError, canCheck, canClaim, isEditable } from "./handles.ts";

async function settle(): Promise<void> {
  for (let index = 0; index < 20; index += 1) {
    await new Promise((resolve) => setImmediate(resolve));
  }
}

class FakeAccount implements HandleAccount {
  minimum = 3;
  times = new Map<string, Date>();
  current: string | undefined;
  others: string[] = [];
  claims: string[] = [];
  error: HandleError | undefined;
  checkError: HandleError | undefined;
  recordFailures = 0;
  /** Free handles per requested name; undefined answers `[name]`. */
  answers: readonly string[] | undefined;
  hold: Promise<void> | undefined;
  claimStarted = false;

  claimTimes(): ReadonlyMap<string, Date> {
    return this.times;
  }

  handles() {
    return Promise.resolve({
      ...(this.current === undefined ? {} : { current: this.current }),
      others: this.others,
    });
  }

  availableHandles(name: string): Promise<readonly string[]> {
    if (this.checkError !== undefined)
      return Promise.reject(new HandleFailure(this.checkError));
    return Promise.resolve(this.answers ?? [name]);
  }

  async claimHandle(handle: string): Promise<void> {
    this.claims.push(handle);
    this.claimStarted = true;
    if (this.hold !== undefined) await this.hold;
    if (this.error !== undefined) throw new HandleFailure(this.error);
    this.current = handle;
  }

  recordClaim(handle: string, at: Date): void {
    if (this.recordFailures > 0) {
      this.recordFailures -= 1;
      throw new Error("disk full");
    }
    this.times.set(handle, at);
  }
}

function model(
  account: FakeAccount,
  options: Partial<HandleModelOptions> = {},
): { model: HandleModel; finished: () => number } {
  let finished = 0;
  const handleModel = new HandleModel({
    account,
    mode: "choose",
    isSessionCurrent: () => true,
    finish: () => {
      finished += 1;
    },
    ...options,
  });
  return { model: handleModel, finished: () => finished };
}

const codes = ["abc#0001", "abc#0002", "abc#0003", "abc#0004"];

test("suggestions show three at a time; taken and tooSoon claims (suggestionsAndClaimFailures)", async () => {
  const account = new FakeAccount();
  account.answers = codes;
  const { model: store } = model(account);
  await store.load();
  store.changeInput("abc");
  store.check();
  await settle();
  let status = store.getState().status;
  assert.equal(status.kind, "suggestions");
  assert.deepEqual(status.kind === "suggestions" ? status.shown : 0, 3);
  store.showMore();
  status = store.getState().status;
  assert.equal(status.kind === "suggestions" ? status.shown : 0, 4);

  account.error = "taken";
  store.claim();
  await settle();
  const state = store.getState();
  assert.equal(state.notice?.kind, "taken");
  assert.equal(
    state.status.kind === "suggestions" ? state.status.selected : undefined,
    "abc#0002",
  );
  assert.ok(
    state.status.kind === "suggestions" &&
      !state.status.handles.includes("abc#0001"),
  );
  assert.equal(account.claims.length, 1);

  account.error = "tooSoon";
  store.claim();
  await settle();
  assert.equal(store.getState().notice?.kind, "tooSoon");
  assert.equal(isEditable(store.getState()), false, "blocked");
});

test("the handle limit locks the editor (claimLimitLocksEditor)", async () => {
  const account = new FakeAccount();
  account.error = "limitReached";
  const { model: store } = model(account);
  await store.load();
  store.changeInput("abc");
  store.check();
  await settle();
  store.claim();
  await settle();
  assert.equal(store.getState().notice?.kind, "limitReached");
  assert.equal(isEditable(store.getState()), false);
  store.changeInput("different");
  assert.equal(store.getState().input, "");
});

test("editing while the first read runs stays usable (editingDuringInitialHandleLoadRemainsUsable)", async () => {
  const account = new FakeAccount();
  let complete = () => {};
  account.handles = () =>
    new Promise((resolve) => {
      complete = () => {
        resolve({ others: [] });
      };
    });
  const { model: store } = model(account);
  const loading = store.load();
  store.changeInput("alice");
  assert.equal(store.getState().loading, true);
  complete();
  await loading;
  assert.equal(store.getState().loading, false);
  assert.equal(store.getState().input, "alice");
  assert.equal(canCheck(store.getState()), true);
});

test("a failed claim record retries only the local save (failedClaimRecordRetriesOnlyLocalSave)", async () => {
  const account = new FakeAccount();
  account.answers = codes;
  account.recordFailures = 1;
  const { model: store, finished } = model(account);
  await store.load();
  store.changeInput("abc");
  store.check();
  await settle();
  store.claim();
  await settle();
  let state = store.getState();
  assert.equal(state.current, "abc#0001");
  assert.deepEqual(state.notice, { kind: "recordFailed", handle: "abc#0001" });
  assert.equal(isEditable(state), false);
  assert.equal(finished(), 0);
  assert.equal(canRetryNotice(state.notice), true);
  store.retry();
  state = store.getState();
  assert.equal(state.notice, undefined);
  assert.equal(account.claims.length, 1, "no second SDK claim");
  assert.ok(account.times.has("abc#0001"));
  assert.equal(finished(), 1);
});

test("a successful claim records its time after the screen closes (successfulClaimPersistsAfterRouteCloses)", async () => {
  const account = new FakeAccount();
  let release = () => {};
  account.hold = new Promise((resolve) => {
    release = resolve;
  });
  const { model: store, finished } = model(account);
  store.changeInput("abc");
  store.check();
  await settle();
  store.claim();
  await settle();
  assert.equal(account.claimStarted, true);
  store.dispose();
  release();
  await settle();
  assert.ok(account.times.has("abc"));
  assert.equal(finished(), 0);
});

test("a claim after the session ended records nothing", async () => {
  const account = new FakeAccount();
  let current = true;
  const { model: store } = model(account, { isSessionCurrent: () => current });
  store.changeInput("abc");
  store.check();
  await settle();
  let release = () => {};
  account.hold = new Promise((resolve) => {
    release = resolve;
  });
  store.claim();
  await settle();
  current = false;
  release();
  await settle();
  assert.equal(account.times.size, 0);
});

test("a choose claim finishes without saving Skip (successfulChooseClaimBypassesFailingSkipPreference)", async () => {
  const account = new FakeAccount();
  let skipWrites = 0;
  const { model: store, finished } = model(account, {
    persistSkip: () => {
      skipWrites += 1;
      throw new Error("no");
    },
  });
  await store.load();
  store.changeInput("claimme");
  store.check();
  await settle();
  assert.equal(canClaim(store.getState()), true);
  store.claim();
  await settle();
  assert.deepEqual(account.claims, ["claimme"]);
  assert.ok(account.times.has("claimme"));
  assert.equal(skipWrites, 0);
  assert.equal(finished(), 1);
  assert.equal(store.getState().notice, undefined);
  assert.equal(store.getState().confirm, undefined, "choose mode never asks");
});

test("an existing handle finishes Choose handle on load (existingHandleFinishesWithoutSavingSkip)", async () => {
  const account = new FakeAccount();
  account.current = "alreadyclaimed";
  let skipWrites = 0;
  const { model: store, finished } = model(account, {
    persistSkip: () => {
      skipWrites += 1;
    },
  });
  await store.load();
  assert.equal(finished(), 1);
  assert.equal(skipWrites, 0);

  const change = model(account, { mode: "change" });
  await change.model.load();
  assert.equal(change.finished(), 0, "Change handle stays open");
  assert.equal(change.model.getState().current, "alreadyclaimed");
});

test("a failed skip retries the preference without claiming (explicitSkipFailureRetriesPreferenceWithoutClaiming)", () => {
  const account = new FakeAccount();
  let skipWrites = 0;
  let fail = true;
  const { model: store, finished } = model(account, {
    persistSkip: () => {
      skipWrites += 1;
      if (fail) throw new Error("no");
    },
  });
  store.skip();
  assert.equal(skipWrites, 1);
  assert.deepEqual(store.getState().notice, { kind: "skipFailed" });
  assert.equal(finished(), 0);
  fail = false;
  store.retry();
  assert.equal(skipWrites, 2);
  assert.equal(store.getState().notice, undefined);
  assert.equal(finished(), 1);
  assert.equal(account.claims.length, 0);
});

test("a stale skip neither saves nor finishes (staleSkipDoesNotSaveOrNavigate)", () => {
  for (const closeRoute of [false, true]) {
    const account = new FakeAccount();
    let writes = 0;
    const { model: store, finished } = model(account, {
      isSessionCurrent: () => closeRoute,
      persistSkip: () => {
        writes += 1;
      },
    });
    if (closeRoute) store.dispose();
    store.skip();
    store.retry();
    assert.equal(writes, 0);
    assert.equal(finished(), 0);
    assert.equal(store.getState().notice, undefined);
  }
});

test("an unchanged input keeps an available claim; change mode confirms (semanticNoOpCommitPreservesAvailableClaim)", async () => {
  for (const input of ["abc", "ABC"]) {
    const account = new FakeAccount();
    const { model: store } = model(account, { mode: "change" });
    await store.load();
    store.changeInput("abc");
    store.check();
    await settle();
    assert.equal(canClaim(store.getState()), true);
    const available = store.getState();
    store.changeInput(input);
    assert.equal(store.getState(), available);
    store.claim();
    assert.equal(store.getState().confirm, "abc");
    assert.equal(account.claims.length, 0);
    store.changeInput("abcd");
    assert.equal(store.getState().input, "abcd");
    assert.equal(store.getState().confirm, undefined);
    assert.equal(canClaim(store.getState()), false);
  }
});

test("change mode: the current handle is a problem, an own handle confirms, and a claim updates the lists", async () => {
  const account = new FakeAccount();
  account.current = "nick";
  account.others = ["nickm"];
  account.times.set("nick", new Date(Date.UTC(2026, 9, 18)));
  const { model: store, finished } = model(account, { mode: "change" });
  await store.load();
  assert.deepEqual(store.getState().currentSince, account.times.get("nick"));
  store.changeInput("nick");
  store.check();
  assert.equal(store.getState().problem, "current");
  store.changeInput("nickm");
  store.check();
  assert.equal(store.getState().confirm, "nickm");
  store.cancelClaim();
  assert.equal(store.getState().confirm, undefined);
  store.switchBack("nickm");
  assert.equal(store.getState().confirm, "nickm");
  store.confirmClaim();
  assert.equal(store.getState().claiming, "nickm");
  await settle();
  const state = store.getState();
  assert.equal(state.current, "nickm");
  assert.deepEqual(state.others, ["nick"]);
  assert.equal(state.input, "");
  assert.equal(finished(), 1);
});

test("a not-allowed claim of an old handle says it cannot be switched back to", async () => {
  const account = new FakeAccount();
  account.current = "nick";
  account.others = ["ni"];
  account.error = "notAllowed";
  const { model: store } = model(account, { mode: "change" });
  await store.load();
  store.switchBack("ni");
  store.confirmClaim();
  await settle();
  assert.deepEqual(store.getState().notice, {
    kind: "switchBackBlocked",
    handle: "ni",
    shorter: true,
  });
  store.changeInput("other");
  store.check();
  await settle();
  store.claim();
  store.confirmClaim();
  await settle();
  assert.deepEqual(store.getState().notice, {
    kind: "notAllowed",
    name: "other",
  });
  assert.equal(canCheck(store.getState()), false, "same name stays blocked");
});

test("check failures: invalid argument marks the input, others offer Retry", async () => {
  const account = new FakeAccount();
  account.checkError = "invalidArgument";
  const { model: store } = model(account);
  await store.load();
  store.changeInput("abc");
  store.check();
  await settle();
  assert.equal(store.getState().problem, "invalidCharacters");
  store.changeInput("abcd");
  account.checkError = "failed";
  store.check();
  await settle();
  assert.deepEqual(store.getState().notice, { kind: "checkFailed" });
  account.checkError = undefined;
  store.retry();
  await settle();
  assert.equal(store.getState().status.kind, "available");
});

test("a late check result is dropped after an edit", async () => {
  const account = new FakeAccount();
  const { model: store } = model(account);
  await store.load();
  store.changeInput("abc");
  store.check();
  store.changeInput("abcd");
  await settle();
  assert.equal(store.getState().status.kind, "editing");
});

test("the server minimum is used only inside 1-27", () => {
  const account = new FakeAccount();
  account.minimum = 5;
  assert.equal(model(account).model.getState().minimum, 5);
  account.minimum = 40;
  assert.equal(model(account).model.getState().minimum, 3);
});
