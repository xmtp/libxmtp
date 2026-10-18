import assert from "node:assert/strict";
import { test } from "node:test";

import {
  type ConversationDetails,
  type DetailsMember,
  type DetailsMutation,
  DetailsStore,
  type GroupQueries,
  initialLoadError,
} from "./details.ts";
import {
  type GroupRole,
  type PermissionWrite,
  permissionPresetWrites,
} from "./permissions.ts";
import { RecipientError } from "./recipients.ts";

/** Waits until every queued job and timer callback has run. */
const tick = (): Promise<void> =>
  new Promise((resolve) => {
    setImmediate(resolve);
  });

async function eventually(condition: () => boolean): Promise<void> {
  for (let index = 0; index < 1000; index += 1) {
    if (condition()) return;
    await tick();
  }
  assert.fail("condition never held");
}

/** A gate that holds callers until `releaseAll`. */
class Gate {
  waiters: (() => void)[] = [];

  readonly wait = (): Promise<void> =>
    new Promise((resolve) => {
      this.waiters.push(resolve);
    });

  releaseAll(): number {
    const pending = this.waiters;
    this.waiters = [];
    for (const resolve of pending) resolve();
    return pending.length;
  }
}

class FakeQueries implements GroupQueries {
  writes: PermissionWrite[] = [];
  mutations: DetailsMutation[] = [];
  loads = 0;
  failureAt: number | undefined;
  rejectMutation = false;
  loadFailure = false;
  pendingLeave = false;
  recipientError: RecipientError | undefined;
  ownRole: GroupRole = "superAdmin";
  ownPresent = true;
  currentPolicies: readonly PermissionWrite[] | undefined;
  members: DetailsMember[] | undefined;
  activeLoads = 0;
  maximumActiveLoads = 0;
  holdLoads: Gate | undefined;
  /** Lookups that wait for a gate, by input. */
  resolveGates = new Map<string, Gate>();
  /** Lookups that fail (after their gate), by input. */
  failingInputs = new Set<string>();

  async load(): Promise<ConversationDetails> {
    this.loads += 1;
    this.activeLoads += 1;
    this.maximumActiveLoads = Math.max(
      this.maximumActiveLoads,
      this.activeLoads,
    );
    try {
      if (this.holdLoads !== undefined) await this.holdLoads.wait();
      if (this.loadFailure) throw new Error("load failed");
      const members: DetailsMember[] = this.members ?? [
        ...(this.ownPresent
          ? [{ id: "own", otherHandles: [], role: this.ownRole }]
          : []),
        { id: "other", otherHandles: [], role: "superAdmin" },
      ];
      return {
        record: { id: "group", kind: "group" },
        kind: "group",
        description: "",
        members,
        policies: this.currentPolicies ?? [...this.writes],
        preset: "Custom",
        retentionNs: 0n,
        pendingLeave: this.pendingLeave,
      };
    } finally {
      this.activeLoads -= 1;
    }
  }

  async resolve(input: string): Promise<DetailsMember> {
    if (this.recipientError !== undefined) throw this.recipientError;
    await this.resolveGates.get(input)?.wait();
    if (this.failingInputs.has(input)) {
      throw new RecipientError("handleNotFound");
    }
    return { id: input, otherHandles: [], role: "member" };
  }

  mutate(mutation: DetailsMutation): Promise<void> {
    this.mutations.push(mutation);
    return this.rejectMutation
      ? Promise.reject(new Error("rejected"))
      : Promise.resolve();
  }

  permission(write: PermissionWrite): Promise<void> {
    if (this.writes.length + 1 === this.failureAt) {
      return Promise.reject(new Error("write failed"));
    }
    this.writes.push(write);
    return Promise.resolve();
  }
}

function makeStore(
  fake: FakeQueries,
  options: { pause?: () => Promise<void>; admit?: () => boolean } = {},
): DetailsStore {
  return new DetailsStore(fake, "own", {
    pause: options.pause ?? (() => new Promise(() => {})),
    debounce: () => Promise.resolve(),
    ...(options.admit === undefined ? {} : { admit: options.admit }),
  });
}

async function finish(store: DetailsStore): Promise<void> {
  await eventually(() => !store.getState().busy);
}

test("a preset writes nine policies in order, then reloads once", async () => {
  const fake = new FakeQueries();
  const store = makeStore(fake);
  store.send({ kind: "refresh" });
  await finish(store);
  fake.loads = 0;
  store.send({ kind: "preset", adminOnly: true });
  await finish(store);
  assert.deepEqual(fake.writes, [
    { kind: "addMember", policy: "admin" },
    { kind: "removeMember", policy: "admin" },
    { kind: "addAdmin", policy: "superAdmin" },
    { kind: "removeAdmin", policy: "superAdmin" },
    { kind: "updateMetadata", policy: "admin", field: "name" },
    { kind: "updateMetadata", policy: "admin", field: "description" },
    { kind: "updateMetadata", policy: "admin", field: "imageUrl" },
    { kind: "updateMetadata", policy: "admin", field: "appData" },
    { kind: "updateMetadata", policy: "admin", field: "disappearing" },
  ]);
  assert.equal(fake.loads, 1);
  assert.equal(permissionPresetWrites(false)[0]?.policy, "allow");
});

test("a failure at the fifth write reads back the committed policies", async () => {
  const fake = new FakeQueries();
  fake.failureAt = 5;
  const store = makeStore(fake);
  store.send({ kind: "refresh" });
  await finish(store);
  fake.loads = 0;
  store.send({ kind: "preset", adminOnly: false });
  await finish(store);
  assert.equal(fake.writes.length, 4);
  assert.equal(fake.loads, 1);
  assert.deepEqual(store.getState().details?.policies, fake.writes);
  assert.equal(
    store.getState().error,
    "Cannot update this conversation. Try again.",
  );
});

for (const role of ["member", "admin"] as const) {
  test(`a preset never starts for role ${role}`, async () => {
    const fake = new FakeQueries();
    fake.ownRole = role;
    fake.currentPolicies = permissionPresetWrites(false);
    const store = makeStore(fake);
    store.send({ kind: "refresh" });
    await finish(store);
    const details = store.getState().details;
    assert.equal(store.canChangePermissionPreset, false);
    let notified = 0;
    const stop = store.subscribe(() => {
      notified += 1;
    });
    store.send({ kind: "preset", adminOnly: true });
    stop();
    assert.equal(notified, 0);
    assert.equal(store.getState().busy, false);
    await finish(store);
    assert.deepEqual(fake.writes, []);
    assert.equal(fake.loads, 1);
    assert.equal(store.getState().details, details);
  });
}

test("a preset needs known own membership", async () => {
  const fake = new FakeQueries();
  fake.ownPresent = false;
  const store = makeStore(fake);
  store.send({ kind: "preset", adminOnly: false });
  assert.equal(store.getState().busy, false);
  store.send({ kind: "refresh" });
  await finish(store);
  store.send({ kind: "preset", adminOnly: false });
  assert.equal(store.getState().busy, false);
  assert.deepEqual(fake.writes, []);
});

test("the preset guard does not restrict other mutations", async () => {
  const fake = new FakeQueries();
  fake.ownRole = "member";
  const store = makeStore(fake);
  store.send({ kind: "refresh" });
  await finish(store);
  store.send({
    kind: "mutate",
    mutation: { kind: "edit", name: "Name", description: "Text" },
  });
  await finish(store);
  assert.equal(fake.mutations.length, 1);
  assert.equal(store.getState().error, undefined);
});

test("a rejected add leaves no pending member", async () => {
  const fake = new FakeQueries();
  fake.rejectMutation = true;
  const store = makeStore(fake);
  store.send({ kind: "mutate", mutation: { kind: "add", input: "inbox" } });
  await finish(store);
  assert.equal(store.getState().pendingMembers.size, 0);
  assert.notEqual(store.getState().error, undefined);
  assert.equal(fake.loads, 1);
});

test("an add resolves the input first and writes the inbox ID", async () => {
  const fake = new FakeQueries();
  const store = makeStore(fake);
  store.send({ kind: "mutate", mutation: { kind: "add", input: "@sam" } });
  await finish(store);
  assert.deepEqual(fake.mutations, [{ kind: "add", input: "@sam" }]);
  assert.equal(store.getState().pendingMembers.has("@sam"), true);
});

test("recipient errors keep their specific copy", async () => {
  for (const problem of ["invalidAddress", "handleNotFound"] as const) {
    const fake = new FakeQueries();
    fake.recipientError = new RecipientError(problem);
    const store = makeStore(fake);
    store.send({ kind: "mutate", mutation: { kind: "add", input: "x" } });
    await finish(store);
    assert.equal(store.getState().error, fake.recipientError.message);
    assert.equal(fake.mutations.length, 0);
  }
});

test("an initial load failure offers Retry", async () => {
  const fake = new FakeQueries();
  fake.loadFailure = true;
  const store = makeStore(fake);
  store.send({ kind: "refresh" });
  await finish(store);
  assert.equal(
    initialLoadError(store.getState()),
    "Cannot update this conversation. Try again.",
  );
  fake.loadFailure = false;
  store.send({ kind: "refresh" });
  await finish(store);
  assert.notEqual(store.getState().details, undefined);
  assert.equal(initialLoadError(store.getState()), undefined);
});

test("a refreshed leave state clears the pending leave", async () => {
  const fake = new FakeQueries();
  fake.pendingLeave = true;
  const store = makeStore(fake);
  store.send({ kind: "refresh" });
  await finish(store);
  assert.equal(store.getState().pendingLeave, true);
  fake.pendingLeave = false;
  store.send({ kind: "refresh" });
  await finish(store);
  assert.equal(store.getState().pendingLeave, false);
});

test("a removed member ends pending role polling", async () => {
  const fake = new FakeQueries();
  fake.members = [];
  const store = makeStore(fake);
  store.send({
    kind: "mutate",
    mutation: { kind: "admin", id: "removed", enabled: true },
  });
  await finish(store);
  assert.equal(store.getState().pendingRoles.size, 0);
});

test("pending changes poll until the group confirms them", async () => {
  const ticks = new Gate();
  const fake = new FakeQueries();
  fake.members = [{ id: "own", otherHandles: [], role: "superAdmin" }];
  const store = makeStore(fake, { pause: ticks.wait });
  store.send({ kind: "mutate", mutation: { kind: "add", input: "first" } });
  await eventually(() => ticks.waiters.length === 1);
  assert.equal(store.getState().pendingMembers.size, 1);
  ticks.releaseAll();
  await eventually(() => ticks.waiters.length === 1);
  assert.equal(fake.loads, 2);
  fake.members = [
    ...fake.members,
    { id: "first", otherHandles: [], role: "member" },
  ];
  ticks.releaseAll();
  await eventually(() => store.getState().pendingMembers.size === 0);
  await tick();
  assert.equal(ticks.waiters.length, 0);
  assert.equal(fake.loads, 3);
});

test("several pending actions share one poller and one read", async () => {
  const ticks = new Gate();
  const fake = new FakeQueries();
  fake.members = [];
  const store = makeStore(fake, { pause: ticks.wait });
  store.send({ kind: "mutate", mutation: { kind: "add", input: "first" } });
  await eventually(() => ticks.waiters.length === 1);
  for (const name of ["second", "third"]) {
    store.send({ kind: "mutate", mutation: { kind: "add", input: name } });
    await finish(store);
  }
  assert.equal(ticks.waiters.length, 1);
  assert.equal(store.getState().pendingMembers.size, 3);
  const loads = new Gate();
  fake.holdLoads = loads;
  ticks.releaseAll();
  await eventually(() => loads.waiters.length === 1);
  assert.equal(fake.maximumActiveLoads, 1);
  loads.releaseAll();
});

test("a disposed store stops polling and drops results", async () => {
  const ticks = new Gate();
  const fake = new FakeQueries();
  fake.members = [];
  const store = makeStore(fake, { pause: ticks.wait });
  store.send({ kind: "mutate", mutation: { kind: "add", input: "first" } });
  await eventually(() => ticks.waiters.length === 1);
  const loads = fake.loads;
  store.dispose();
  ticks.releaseAll();
  await tick();
  await tick();
  assert.equal(fake.loads, loads);
  assert.equal(ticks.waiters.length, 0);
});

test("a stale session drops a load result", async () => {
  let valid = true;
  const fake = new FakeQueries();
  const loads = new Gate();
  fake.holdLoads = loads;
  const store = makeStore(fake, { admit: () => valid });
  store.send({ kind: "refresh" });
  await eventually(() => loads.waiters.length === 1);
  valid = false;
  loads.releaseAll();
  await tick();
  await tick();
  assert.equal(store.getState().details, undefined);
});

test("a recipient preview shows the newest input only", async () => {
  const debounce = new Gate();
  const fake = new FakeQueries();
  const store = new DetailsStore(fake, "own", { debounce: debounce.wait });
  store.resolvePreview("first");
  store.resolvePreview("second");
  debounce.releaseAll();
  await eventually(() => store.getState().recipient !== undefined);
  assert.equal(store.getState().recipient?.id, "second");
  store.resolvePreview("  ");
  assert.equal(store.getState().recipient, undefined);
  assert.equal(debounce.waiters.length, 0);
});

test("a slow older lookup cannot replace a newer one", async () => {
  const fake = new FakeQueries();
  const slow = new Gate();
  fake.resolveGates.set("first", slow);
  const store = makeStore(fake);
  store.resolvePreview("first");
  await tick();
  store.resolvePreview("second");
  await eventually(() => store.getState().recipient !== undefined);
  assert.equal(store.getState().recipient?.id, "second");
  slow.releaseAll();
  await tick();
  await tick();
  assert.equal(store.getState().recipient?.id, "second");
});

test("a recipient preview failure shows its copy", async () => {
  const fake = new FakeQueries();
  fake.recipientError = new RecipientError("handleNotFound");
  const store = makeStore(fake);
  store.resolvePreview("nobody");
  await eventually(() => store.getState().error !== undefined);
  assert.equal(store.getState().error, "No XMTP account has this handle.");
});

test("a settled poller restarts for the next pending action", async () => {
  const ticks = new Gate();
  const fake = new FakeQueries();
  fake.members = [];
  const store = makeStore(fake, { pause: ticks.wait });
  store.send({ kind: "mutate", mutation: { kind: "add", input: "first" } });
  await eventually(() => ticks.waiters.length === 1);
  fake.members = [{ id: "first", otherHandles: [], role: "member" }];
  ticks.releaseAll();
  await eventually(() => store.getState().pendingMembers.size === 0);
  await tick();
  assert.equal(ticks.waiters.length, 0);
  store.send({ kind: "mutate", mutation: { kind: "add", input: "second" } });
  await eventually(() => ticks.waiters.length === 1);
  assert.equal(store.getState().pendingMembers.has("second"), true);
  store.dispose();
  ticks.releaseAll();
});

test("a confirmed removal clears and ends polling", async () => {
  const ticks = new Gate();
  const fake = new FakeQueries();
  const store = makeStore(fake, { pause: ticks.wait });
  store.send({ kind: "mutate", mutation: { kind: "remove", id: "other" } });
  await eventually(() => ticks.waiters.length === 1);
  assert.equal(store.getState().pendingRemoval.has("other"), true);
  fake.members = [{ id: "own", otherHandles: [], role: "superAdmin" }];
  ticks.releaseAll();
  await eventually(() => store.getState().pendingRemoval.size === 0);
  await tick();
  assert.equal(ticks.waiters.length, 0);
});

test("a slow older failed lookup cannot set the error", async () => {
  const fake = new FakeQueries();
  const slow = new Gate();
  fake.resolveGates.set("first", slow);
  fake.failingInputs.add("first");
  const store = makeStore(fake);
  store.resolvePreview("first");
  await eventually(() => slow.waiters.length === 1);
  store.resolvePreview("second");
  await eventually(() => store.getState().recipient !== undefined);
  slow.releaseAll();
  await tick();
  await tick();
  assert.equal(store.getState().error, undefined);
  assert.equal(store.getState().recipient?.id, "second");
});

test("losing admission while an add resolves writes nothing", async () => {
  let valid = true;
  const fake = new FakeQueries();
  const lookup = new Gate();
  fake.resolveGates.set("@sam", lookup);
  const store = makeStore(fake, { admit: () => valid });
  store.send({ kind: "mutate", mutation: { kind: "add", input: "@sam" } });
  await eventually(() => lookup.waiters.length === 1);
  valid = false;
  lookup.releaseAll();
  await tick();
  await tick();
  assert.deepEqual(fake.mutations, []);
  assert.equal(store.getState().pendingMembers.size, 0);
});
