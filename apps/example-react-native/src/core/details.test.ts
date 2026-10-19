import assert from "node:assert/strict";
import { test } from "node:test";

import {
  type ConversationDetails,
  DetailsCopy,
  type DetailsMember,
  type DetailsMutation,
  DetailsStore,
  type GroupQueries,
  initialLoadError,
  memberPending,
} from "./details.ts";
import {
  type GroupRole,
  type PermissionWrite,
  permissionPresetWrites,
} from "./permissions.ts";
import { RecipientCopy, RecipientError } from "./recipients.ts";
import { settle } from "./testing.ts";

async function eventually(condition: () => boolean): Promise<void> {
  await settle();
  assert.ok(condition(), "condition did not hold after one turn");
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

const member = (id: string, role: GroupRole = "member"): DetailsMember => ({
  id,
  otherHandles: [],
  role,
});

class FakeQueries implements GroupQueries {
  writes: PermissionWrite[] = [];
  mutations: DetailsMutation[] = [];
  loads = 0;
  /** Inputs that reached `resolve`. */
  lookups: string[] = [];
  /** The first write this matches fails; later writes succeed. */
  failWrite: ((write: PermissionWrite) => boolean) | undefined;
  rejectMutation = false;
  loadFailure = false;
  pendingLeave = false;
  recipientError: Error | undefined;
  ownRole: GroupRole = "superAdmin";
  members: DetailsMember[] | undefined;
  holdLoads: Gate | undefined;
  /** The inbox ID an input resolves to (default: the input). */
  inboxes = new Map<string, string>();
  /** Lookups that wait for a gate, by input. */
  resolveGates = new Map<string, Gate>();
  /** Lookups that fail (after their gate), by input. */
  failingInputs = new Set<string>();

  async load(): Promise<ConversationDetails> {
    this.loads += 1;
    if (this.holdLoads !== undefined) await this.holdLoads.wait();
    if (this.loadFailure) throw new Error("load failed");
    return {
      record: { id: "group", kind: "group" },
      kind: "group",
      description: "",
      members: this.members ?? [
        member("own", this.ownRole),
        member("other", "superAdmin"),
      ],
      policies: [...this.writes],
      preset: "Custom",
      retentionNs: 0n,
      pendingLeave: this.pendingLeave,
    };
  }

  async resolve(input: string): Promise<DetailsMember> {
    this.lookups.push(input);
    if (this.recipientError !== undefined) throw this.recipientError;
    await this.resolveGates.get(input)?.wait();
    if (this.failingInputs.has(input)) {
      throw new RecipientError("handleNotFound");
    }
    return member(this.inboxes.get(input) ?? input);
  }

  mutate(mutation: DetailsMutation): Promise<void> {
    this.mutations.push(mutation);
    return this.rejectMutation
      ? Promise.reject(new Error("rejected"))
      : Promise.resolve();
  }

  permission(write: PermissionWrite): Promise<void> {
    if (this.failWrite?.(write) === true) {
      this.failWrite = undefined;
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

test("a preset writes its policies in order, then shows the group's policies", async () => {
  const fake = new FakeQueries();
  const store = makeStore(fake);
  store.send({ kind: "refresh" });
  await finish(store);
  store.send({ kind: "preset", adminOnly: true });
  await finish(store);
  assert.deepEqual(fake.writes, permissionPresetWrites(true));
  assert.deepEqual(
    store.getState().details?.policies,
    permissionPresetWrites(true),
  );
});

test("a failed preset write stops the preset and shows the committed policies", async () => {
  const fake = new FakeQueries();
  fake.failWrite = (write) => write.kind === "updateMetadata";
  const store = makeStore(fake);
  store.send({ kind: "refresh" });
  await finish(store);
  store.send({ kind: "preset", adminOnly: false });
  await finish(store);
  assert.ok(fake.writes.length > 0);
  assert.ok(!fake.writes.some((write) => write.kind === "updateMetadata"));
  assert.deepEqual(store.getState().details?.policies, fake.writes);
  assert.equal(store.getState().error, DetailsCopy.updateFailed);
  assert.equal(initialLoadError(store.getState()), undefined);
});

test("a preset never starts without a loaded super admin role, and other edits still run", async () => {
  const fake = new FakeQueries();
  fake.ownRole = "member";
  const store = makeStore(fake);
  store.send({ kind: "preset", adminOnly: true });
  assert.equal(store.getState().busy, false);
  store.send({ kind: "refresh" });
  await finish(store);
  assert.equal(store.canChangePermissionPreset, false);
  let notified = 0;
  const stop = store.subscribe(() => {
    notified += 1;
  });
  store.send({ kind: "preset", adminOnly: true });
  stop();
  assert.equal(notified, 0);
  await finish(store);
  assert.deepEqual(fake.writes, []);
  // The preset guard does not block other edits.
  store.send({
    kind: "mutate",
    mutation: { kind: "edit", name: "Name", description: "Text" },
  });
  await finish(store);
  assert.equal(fake.mutations.length, 1);
  assert.equal(store.getState().error, undefined);
});

test("a failed add leaves no pending member and shows why", async () => {
  const fake = new FakeQueries();
  fake.recipientError = new RecipientError("invalidAddress");
  const store = makeStore(fake);
  store.send({ kind: "mutate", mutation: { kind: "add", input: "x" } });
  await finish(store);
  assert.equal(store.getState().error, RecipientCopy.invalidAddress);
  assert.equal(fake.mutations.length, 0);
  fake.recipientError = undefined;
  fake.rejectMutation = true;
  store.send({ kind: "mutate", mutation: { kind: "add", input: "inbox" } });
  await finish(store);
  assert.equal(store.getState().pendingMembers.size, 0);
  assert.equal(store.getState().error, DetailsCopy.updateFailed);
  assert.notEqual(store.getState().details, undefined);
});

test("an add resolves the input first and writes the inbox ID", async () => {
  const fake = new FakeQueries();
  fake.inboxes.set("@sam", "inbox-sam");
  const store = makeStore(fake);
  store.send({ kind: "mutate", mutation: { kind: "add", input: "@sam" } });
  await finish(store);
  assert.deepEqual(fake.mutations, [{ kind: "add", input: "inbox-sam" }]);
  assert.equal(store.getState().pendingMembers.has("inbox-sam"), true);
});

test("an initial load failure offers Retry, and a retry clears the error", async () => {
  const fake = new FakeQueries();
  fake.loadFailure = true;
  const store = makeStore(fake);
  store.send({ kind: "refresh" });
  await finish(store);
  assert.equal(initialLoadError(store.getState()), DetailsCopy.updateFailed);
  fake.loadFailure = false;
  store.send({ kind: "refresh" });
  await finish(store);
  assert.notEqual(store.getState().details, undefined);
  assert.equal(store.getState().error, undefined);
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

test("a role change polls until the group shows it or the member leaves", async () => {
  const ticks = new Gate();
  const fake = new FakeQueries();
  fake.members = [
    member("own", "superAdmin"),
    member("target"),
    member("gone"),
  ];
  const store = makeStore(fake, { pause: ticks.wait });
  for (const id of ["target", "gone"]) {
    store.send({
      kind: "mutate",
      mutation: { kind: "admin", id, enabled: true },
    });
    await finish(store);
  }
  await eventually(() => ticks.waiters.length === 1);
  assert.ok(memberPending(store.getState(), "target"));
  assert.ok(memberPending(store.getState(), "gone"));
  fake.members = [member("own", "superAdmin"), member("target", "admin")];
  ticks.releaseAll();
  await eventually(() => !memberPending(store.getState(), "target"));
  assert.ok(!memberPending(store.getState(), "gone"));
  await settle();
  assert.equal(ticks.waiters.length, 0);
});

test("pending changes poll until the group confirms them, and a new change polls again", async () => {
  const ticks = new Gate();
  const fake = new FakeQueries();
  fake.members = [member("own", "superAdmin"), member("other")];
  const store = makeStore(fake, { pause: ticks.wait });
  store.send({ kind: "mutate", mutation: { kind: "remove", id: "other" } });
  await eventually(() => ticks.waiters.length === 1);
  store.send({ kind: "mutate", mutation: { kind: "add", input: "first" } });
  await finish(store);
  // Not confirmed yet: polling goes on.
  ticks.releaseAll();
  await eventually(() => ticks.waiters.length === 1);
  assert.ok(store.getState().pendingRemoval.has("other"));
  assert.ok(store.getState().pendingMembers.has("first"));
  // Confirmed: both clear and polling stops.
  fake.members = [member("own", "superAdmin"), member("first")];
  ticks.releaseAll();
  await eventually(
    () =>
      store.getState().pendingMembers.size === 0 &&
      store.getState().pendingRemoval.size === 0,
  );
  await settle();
  assert.equal(ticks.waiters.length, 0);
  // A new change starts polling again.
  store.send({ kind: "mutate", mutation: { kind: "add", input: "second" } });
  await eventually(() => ticks.waiters.length === 1);
  assert.equal(store.getState().pendingMembers.has("second"), true);
  store.dispose();
  ticks.releaseAll();
});

test("several pending actions share one poller", async () => {
  const ticks = new Gate();
  const fake = new FakeQueries();
  fake.members = [];
  const store = makeStore(fake, { pause: ticks.wait });
  for (const name of ["first", "second", "third"]) {
    store.send({ kind: "mutate", mutation: { kind: "add", input: name } });
    await finish(store);
  }
  await settle();
  assert.equal(ticks.waiters.length, 1);
  assert.equal(store.getState().pendingMembers.size, 3);
  store.dispose();
  ticks.releaseAll();
});

test("a disposed store stops polling", async () => {
  const ticks = new Gate();
  const fake = new FakeQueries();
  fake.members = [];
  const store = makeStore(fake, { pause: ticks.wait });
  store.send({ kind: "mutate", mutation: { kind: "add", input: "first" } });
  await eventually(() => ticks.waiters.length === 1);
  const loads = fake.loads;
  store.dispose();
  ticks.releaseAll();
  await settle();
  await settle();
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
  await settle();
  await settle();
  assert.equal(store.getState().details, undefined);
});

test("a recipient preview looks up and shows the newest input only", async () => {
  const debounce = new Gate();
  const fake = new FakeQueries();
  const store = new DetailsStore(fake, "own", { debounce: debounce.wait });
  store.resolvePreview("first");
  store.resolvePreview("second");
  debounce.releaseAll();
  await eventually(() => store.getState().recipient !== undefined);
  assert.equal(store.getState().recipient?.id, "second");
  assert.deepEqual(fake.lookups, ["second"]);
  store.resolvePreview("  ");
  assert.equal(store.getState().recipient, undefined);
  assert.equal(debounce.waiters.length, 0);
});

test("a slow older lookup cannot replace a newer result or set an error", async () => {
  const fake = new FakeQueries();
  const slowFound = new Gate();
  const slowFailed = new Gate();
  fake.resolveGates.set("found", slowFound);
  fake.resolveGates.set("failed", slowFailed);
  fake.failingInputs.add("failed");
  const store = makeStore(fake);
  store.resolvePreview("found");
  await eventually(() => slowFound.waiters.length === 1);
  store.resolvePreview("failed");
  await eventually(() => slowFailed.waiters.length === 1);
  store.resolvePreview("newest");
  await eventually(() => store.getState().recipient !== undefined);
  assert.equal(store.getState().recipient?.id, "newest");
  slowFound.releaseAll();
  slowFailed.releaseAll();
  await settle();
  await settle();
  assert.equal(store.getState().recipient?.id, "newest");
  assert.equal(store.getState().error, undefined);
});

test("a recipient preview failure shows its copy", async () => {
  const fake = new FakeQueries();
  fake.recipientError = new RecipientError("handleNotFound");
  const store = makeStore(fake);
  store.resolvePreview("nobody");
  await eventually(() => store.getState().error !== undefined);
  assert.equal(store.getState().error, RecipientCopy.handleNotFound);
  fake.recipientError = new Error("offline");
  store.resolvePreview("somebody");
  // A new input clears the old error at once.
  assert.equal(store.getState().error, undefined);
  await eventually(() => store.getState().error !== undefined);
  assert.equal(store.getState().error, DetailsCopy.recipientNotFound);
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
  await settle();
  await settle();
  assert.deepEqual(fake.mutations, []);
  assert.equal(store.getState().pendingMembers.size, 0);
});
