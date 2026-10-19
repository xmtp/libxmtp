import assert from "node:assert/strict";
import { test } from "node:test";

import {
  type HandleAccount,
  type HandleModelOptions,
  HandleFailure,
  HandleModel,
  canRetryNotice,
} from "./handleModel.ts";
import {
  type HandleError,
  canCheck,
  canClaim,
  isEditable,
  statusPick,
  visibleSuggestions,
} from "./handles.ts";
import { deferred, settle } from "./testing.ts";

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
  options: Partial<HandleModelOptions> & {
    /** Makes "Skip for now" fail to save while it returns true. */
    readonly failSkip?: () => boolean;
  } = {},
) {
  let finished = 0;
  let skipWrites = 0;
  const { failSkip, ...overrides } = options;
  const handleModel = new HandleModel({
    account,
    mode: "choose",
    isSessionCurrent: () => true,
    finish: () => {
      finished += 1;
    },
    persistSkip: () => {
      skipWrites += 1;
      if (failSkip?.() === true) throw new Error("no");
    },
    ...overrides,
  });
  return {
    model: handleModel,
    finished: () => finished,
    skipWrites: () => skipWrites,
  };
}

/** Types `input`, checks it and waits for the answer. */
async function checked(store: HandleModel, input: string): Promise<void> {
  store.changeInput(input);
  store.check();
  await settle();
}

const codes = ["abc#0001", "abc#0002", "abc#0003", "abc#0004"];

test("suggestions show three at a time, and a taken claim offers the rest", async () => {
  const account = new FakeAccount();
  account.answers = codes;
  const { model: store } = model(account);
  await store.load();
  await checked(store, "abc");
  assert.equal(visibleSuggestions(store.getState().status).length, 3);
  store.showMore();
  assert.equal(visibleSuggestions(store.getState().status).length, 4);

  account.error = "taken";
  store.claim();
  await settle();
  const state = store.getState();
  assert.equal(state.notice?.kind, "taken");
  assert.equal(statusPick(state.status), "abc#0002");
  assert.ok(!visibleSuggestions(state.status).includes("abc#0001"));
  assert.equal(account.claims.length, 1);
});

test("tooSoon and the handle limit clear and lock the editor", async () => {
  for (const error of ["tooSoon", "limitReached"] as const) {
    const account = new FakeAccount();
    account.error = error;
    const { model: store } = model(account);
    await store.load();
    await checked(store, "abc");
    store.claim();
    await settle();
    const state = store.getState();
    assert.equal(state.notice?.kind, error);
    assert.equal(state.input, "", error);
    assert.equal(state.problem, undefined, error);
    assert.deepEqual(state.status, { kind: "editing" }, error);
    assert.equal(isEditable(state), false, error);
    store.changeInput("different");
    assert.equal(store.getState().input, "", error);
  }
});

test("editing while the first read runs stays usable", async () => {
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

test("a failed claim record retries only the local save, and Skip waits for it", async () => {
  const account = new FakeAccount();
  account.answers = codes;
  account.recordFailures = 1;
  const { model: store, finished, skipWrites } = model(account);
  await store.load();
  await checked(store, "abc");
  store.claim();
  await settle();
  let state = store.getState();
  assert.equal(state.current, "abc#0001");
  assert.deepEqual(state.notice, { kind: "recordFailed", handle: "abc#0001" });
  assert.equal(isEditable(state), false);
  assert.equal(finished(), 0);
  assert.equal(canRetryNotice(state.notice), true);
  store.skip();
  assert.equal(skipWrites(), 0, "Skip is ignored while the record is unsaved");
  assert.equal(finished(), 0);
  assert.deepEqual(store.getState().notice, state.notice);
  store.retry();
  state = store.getState();
  assert.equal(state.notice, undefined);
  assert.equal(account.claims.length, 1, "no second SDK claim");
  assert.ok(account.times.has("abc#0001"));
  assert.equal(finished(), 1);
});

test("a claim that finishes after the screen closed records its time only while the session is current", async () => {
  const cases = [
    { sessionEnds: false, closeRoute: true, recorded: true },
    { sessionEnds: true, closeRoute: false, recorded: false },
    { sessionEnds: true, closeRoute: true, recorded: false },
  ];
  for (const { sessionEnds, closeRoute, recorded } of cases) {
    const label = JSON.stringify({ sessionEnds, closeRoute });
    const account = new FakeAccount();
    let current = true;
    const { model: store, finished } = model(account, {
      isSessionCurrent: () => current,
    });
    await checked(store, "abc");
    const hold = deferred();
    account.hold = hold.promise;
    store.claim();
    await settle();
    // The screen may close and the session may end while the claim runs.
    if (closeRoute) store.dispose();
    if (sessionEnds) current = false;
    hold.resolve();
    await settle();
    assert.equal(account.times.has("abc"), recorded, label);
    assert.equal(finished(), 0, label);
  }
});

test("a choose claim finishes without saving Skip", async () => {
  const account = new FakeAccount();
  const {
    model: store,
    finished,
    skipWrites,
  } = model(account, {
    failSkip: () => true,
  });
  await store.load();
  await checked(store, "claimme");
  assert.equal(canClaim(store.getState()), true);
  store.claim();
  await settle();
  assert.deepEqual(account.claims, ["claimme"]);
  assert.ok(account.times.has("claimme"));
  assert.equal(skipWrites(), 0);
  assert.equal(finished(), 1);
  assert.equal(store.getState().notice, undefined);
  assert.equal(store.getState().confirm, undefined, "choose mode never asks");
});

test("an existing handle finishes Choose handle on load", async () => {
  const account = new FakeAccount();
  account.current = "alreadyclaimed";
  const { model: store, finished, skipWrites } = model(account);
  await store.load();
  assert.equal(finished(), 1);
  assert.equal(skipWrites(), 0);

  const change = model(account, { mode: "change" });
  await change.model.load();
  assert.equal(change.finished(), 0, "Change handle stays open");
  assert.equal(change.model.getState().current, "alreadyclaimed");
});

test("a failed skip retries the preference without claiming", () => {
  const account = new FakeAccount();
  let fail = true;
  const {
    model: store,
    finished,
    skipWrites,
  } = model(account, {
    failSkip: () => fail,
  });
  store.skip();
  assert.equal(skipWrites(), 1);
  assert.deepEqual(store.getState().notice, { kind: "skipFailed" });
  assert.equal(finished(), 0);
  fail = false;
  store.retry();
  assert.equal(skipWrites(), 2);
  assert.equal(store.getState().notice, undefined);
  assert.equal(finished(), 1);
  assert.equal(account.claims.length, 0);
});

test("a stale skip neither saves nor finishes", () => {
  for (const closeRoute of [false, true]) {
    const account = new FakeAccount();
    const {
      model: store,
      finished,
      skipWrites,
    } = model(account, {
      isSessionCurrent: () => closeRoute,
    });
    if (closeRoute) store.dispose();
    store.skip();
    store.retry();
    assert.equal(skipWrites(), 0);
    assert.equal(finished(), 0);
    assert.equal(store.getState().notice, undefined);
  }
});

test("an unchanged input keeps an available claim; change mode confirms", async () => {
  for (const input of ["abc", "ABC"]) {
    const account = new FakeAccount();
    const { model: store } = model(account, { mode: "change" });
    await store.load();
    await checked(store, "abc");
    assert.equal(canClaim(store.getState()), true);
    const available = store.getState();
    store.changeInput(input);
    assert.deepEqual(store.getState(), available);
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
  await checked(store, "other");
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
  await checked(store, "abc");
  assert.equal(store.getState().problem, "invalidCharacters");
  account.checkError = "failed";
  await checked(store, "abcd");
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
  const cases = [
    [5, 5],
    [1, 1],
    [27, 27],
    [0, 3],
    [28, 3],
    [2.5, 3],
  ] as const;
  for (const [server, minimum] of cases) {
    const account = new FakeAccount();
    account.minimum = server;
    assert.equal(
      model(account).model.getState().minimum,
      minimum,
      String(server),
    );
  }
});

test("load reopens a disposed model (a React effect runs again after its cleanup)", async () => {
  const account = new FakeAccount();
  const { model: store } = model(account);
  store.dispose();
  await store.load();
  assert.equal(store.getState().loading, false);
  store.changeInput("abc");
  assert.equal(store.getState().input, "abc");
});
