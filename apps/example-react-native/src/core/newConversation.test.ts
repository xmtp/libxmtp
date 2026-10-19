import assert from "node:assert/strict";
import { describe, test } from "node:test";

import {
  type IdentityPort,
  NewConversationCopy,
  NewConversationError,
  type NewConversationQueries,
  NewConversationStore,
  type Person,
  addDisabled,
  canContinue,
  initialNewConversationState,
  memberCount,
  personLabel,
  resolveInboxId,
  resolvePerson,
} from "./newConversation.ts";
import { RecipientCopy } from "./recipients.ts";
import { deferred, settle } from "./testing.ts";

const address = "0x" + "ab".repeat(20);
const inbox = "1".repeat(64);

class NotFound extends Error {}

type Answer =
  | "matching"
  | "wrongAddress"
  | "wrongKind"
  | "missingState"
  | "wrongInbox"
  | "failure";

/** An identity port whose `inboxStates` answer each row sets. */
function associationProbe(answer: Answer) {
  const refreshes: boolean[] = [];
  const port: IdentityPort = {
    inboxIdFor: () => Promise.resolve(inbox),
    inboxStates: (_ids, refresh) => {
      refreshes.push(refresh);
      if (answer === "failure") return Promise.reject(new Error("unavailable"));
      if (answer === "missingState") return Promise.resolve([]);
      return Promise.resolve([
        {
          inboxId: answer === "wrongInbox" ? "2".repeat(64) : inbox,
          identities: [
            {
              kind: answer === "wrongKind" ? "handle" : "ethereum",
              identifier:
                answer === "wrongAddress"
                  ? "0x" + "cd".repeat(20)
                  : address.toUpperCase(),
            },
          ],
        },
      ]);
    },
    isNotFound: (error) => error instanceof NotFound,
  };
  return { port, refreshes };
}

async function failure(work: Promise<unknown>): Promise<unknown> {
  try {
    await work;
  } catch (error) {
    return error;
  }
  assert.fail("resolved");
}

describe("address resolution requires its verified association", () => {
  for (const [answer, message] of [
    ["matching", undefined],
    ["wrongAddress", RecipientCopy.addressNotFound],
    ["wrongKind", RecipientCopy.addressNotFound],
    ["missingState", RecipientCopy.addressNotFound],
    ["wrongInbox", RecipientCopy.addressNotFound],
    // A failed verification read stays an error, not "no account".
    ["failure", "unavailable"],
  ] as const) {
    test(answer, async () => {
      const { port, refreshes } = associationProbe(answer);
      if (message === undefined) {
        assert.equal(await resolveInboxId(address, port), inbox);
      } else {
        const error = await failure(resolveInboxId(address, port));
        assert.ok(error instanceof Error);
        assert.equal(error.message, message);
      }
      assert.deepEqual(refreshes, [true]);
    });
  }
});

function lookupPort(
  inboxes: Record<string, string | Error>,
  handles: Record<string, string> = {},
): IdentityPort {
  return {
    inboxIdFor: ({ identifier }) => {
      const found = inboxes[identifier];
      if (found instanceof Error) return Promise.reject(found);
      return Promise.resolve(found);
    },
    inboxStates: (ids, refresh) => {
      if (refresh) {
        return Promise.resolve(
          ids.map((id) => ({
            inboxId: id,
            identities: [{ kind: "ethereum", identifier: address }],
          })),
        );
      }
      return Promise.resolve(
        ids.map((id) => {
          const handle = handles[id];
          return {
            inboxId: id,
            identities: [],
            ...(handle === undefined ? {} : { handle }),
          };
        }),
      );
    },
    isNotFound: (error) => error instanceof NotFound,
  };
}

test("resolves handles, addresses and inbox IDs", async () => {
  const port = lookupPort(
    {
      lena: "a".repeat(64),
      [address]: "b".repeat(64),
      gone: new NotFound(),
      offline: new Error("offline"),
    },
    { ["a".repeat(64)]: "lena" },
  );
  assert.deepEqual(await resolvePerson("@Lena", port), {
    inboxId: "a".repeat(64),
    handle: "lena",
  });
  assert.deepEqual(await resolvePerson(" lena ", port), {
    inboxId: "a".repeat(64),
    handle: "lena",
  });
  assert.deepEqual(
    await resolvePerson(address.toUpperCase().replace("0X", "0x"), port),
    {
      inboxId: "b".repeat(64),
    },
  );
  // An inbox ID is accepted as is (the port has no entry for it).
  assert.deepEqual(await resolvePerson("C".repeat(64), port), {
    inboxId: "c".repeat(64),
  });
  for (const [input, copy] of [
    ["nobody", RecipientCopy.handleNotFound],
    ["gone", RecipientCopy.handleNotFound],
    ["0x" + "e".repeat(40), RecipientCopy.addressNotFound],
    ["0x12", RecipientCopy.invalidAddress],
    ["g".repeat(64), RecipientCopy.invalidInbox],
    ["9lives", RecipientCopy.invalidHandle],
    [" ", RecipientCopy.empty],
  ] as const) {
    const error = await failure(resolvePerson(input, port));
    assert.ok(error instanceof NewConversationError, input);
    assert.equal(error.message, copy, input);
  }
  // Any other lookup failure is not shown as "not found".
  const offline = await failure(resolvePerson("offline", port));
  assert.ok(!(offline instanceof NewConversationError));
});

test("a failed handle read leaves the person without a handle", async () => {
  const port: IdentityPort = {
    inboxIdFor: () => Promise.resolve(inbox),
    inboxStates: () => Promise.reject(new Error("offline")),
    isNotFound: () => false,
  };
  assert.deepEqual(await resolvePerson("lena", port), { inboxId: inbox });
});

test("labels show @handle or a 12-character inbox prefix", () => {
  assert.equal(
    personLabel({ inboxId: inbox, handle: "lena#0412" }),
    "@lena#0412",
  );
  assert.equal(
    personLabel({ inboxId: "0f3e8b21c5aa" + "0".repeat(52) }),
    "0f3e8b21c5aa",
  );
});

test("a group cannot continue before its limit is known", () => {
  const state = {
    ...initialNewConversationState,
    group: true,
    input: "person",
  };
  assert.equal(canContinue(state), false);
  assert.equal(canContinue({ ...state, maximumMembers: 10 }), true);
  assert.equal(
    canContinue({ ...state, maximumMembers: 10, input: " " }),
    false,
  );
  assert.equal(
    canContinue({ ...state, maximumMembers: 10, busy: true }),
    false,
  );
  assert.equal(
    canContinue({ ...state, maximumMembers: 10, loadingPeople: true }),
    false,
  );
  assert.equal(
    canContinue({
      ...initialNewConversationState,
      recipients: [{ inboxId: inbox }],
    }),
    true,
  );
  assert.equal(
    addDisabled({ ...initialNewConversationState, input: " " }),
    true,
  );
  assert.equal(
    addDisabled({ ...initialNewConversationState, input: "a" }),
    false,
  );
  assert.equal(
    memberCount({
      ...initialNewConversationState,
      recipients: [{ inboxId: inbox }],
    }),
    2,
  );
});

interface FakeOptions {
  readonly people?: readonly Person[];
  readonly maximum?: number;
  readonly failPeople?: boolean;
  readonly failLimit?: boolean;
}

type Query = "people" | "maximum" | "create";

class FakeQueries implements NewConversationQueries {
  calls: string[] = [];
  created: Parameters<NewConversationQueries["create"]>[0][] = [];
  options: FakeOptions;
  createFailure: Error | undefined;
  resolveGate: Promise<void> | undefined;
  peopleGate: Promise<void> | undefined;
  readonly #during = new Map<Query, () => void>();

  constructor(options: FakeOptions = {}) {
    this.options = options;
  }

  /** Runs `effect` inside the next `name` call, before it answers. */
  during(name: Query, effect: () => void): void {
    this.#during.set(name, effect);
  }

  #call(name: Query): void {
    this.calls.push(name);
    const effect = this.#during.get(name);
    this.#during.delete(name);
    effect?.();
  }

  async people(): Promise<readonly Person[]> {
    this.#call("people");
    if (this.peopleGate !== undefined) await this.peopleGate;
    if (this.options.failPeople === true) throw new Error("unavailable");
    return this.options.people ?? [];
  }

  maximumMembers(): Promise<number> {
    this.#call("maximum");
    if (this.options.failLimit === true) {
      return Promise.reject(new Error("unavailable"));
    }
    return Promise.resolve(this.options.maximum ?? 250);
  }

  async resolve(input: string): Promise<Person> {
    this.calls.push(`resolve:${input}`);
    if (this.resolveGate !== undefined) await this.resolveGate;
    const value = input.trim().replace(/^@/u, "");
    if (value === "bad")
      throw new NewConversationError(RecipientCopy.invalidHandle);
    if (value === "boom") throw new Error("network");
    return { inboxId: `inbox-${value}`, handle: value };
  }

  create(
    request: Parameters<NewConversationQueries["create"]>[0],
  ): Promise<string> {
    this.#call("create");
    this.created.push(request);
    if (this.createFailure !== undefined)
      return Promise.reject(this.createFailure);
    return Promise.resolve("conversation-1");
  }
}

function makeStore(queries: FakeQueries, admit: () => boolean = () => true) {
  const finished: string[] = [];
  const store = new NewConversationStore(queries, {
    admit,
    finish: (id) => finished.push(id),
  });
  return { store, finished };
}

describe("admission", () => {
  test("stale actions do not call queries", async () => {
    for (const ended of ["session", "dispose"] as const) {
      const queries = new FakeQueries();
      let valid = true;
      const { store } = makeStore(queries, () => valid);
      store.setInput("peer");
      if (ended === "session") valid = false;
      else store.dispose();
      store.load();
      store.add();
      store.next();
      store.create();
      store.setInput("changed");
      store.setName("changed");
      await settle();
      assert.deepEqual(queries.calls, [], ended);
      // A store whose session ended is still on screen: edits do nothing.
      if (ended === "session") {
        assert.equal(store.getState().input, "peer");
        assert.equal(store.getState().name, "");
      }
    }
  });

  test("a load that outlives its session is not applied", async () => {
    const peer: Person = { inboxId: "inbox-peer", handle: "person" };
    const queries = new FakeQueries({ people: [peer], maximum: 10 });
    let valid = true;
    // The session ends during the last read, so only the final check stops it.
    queries.during("maximum", () => {
      valid = false;
    });
    const { store } = makeStore(queries, () => valid);
    store.load();
    await settle();
    assert.deepEqual(store.getState().people, []);
    assert.equal(store.getState().maximumMembers, 0);
  });
});

describe("bootstrap failure survives editing and can retry", () => {
  const peer: Person = { inboxId: "inbox-peer", handle: "person" };
  for (const failing of ["limit", "people"] as const) {
    test(`${failing} fails`, async () => {
      const queries = new FakeQueries({
        failPeople: failing === "people",
        failLimit: failing === "limit",
        maximum: 10,
      });
      const { store } = makeStore(queries);
      store.setGroup(true);
      store.setInput("first");
      store.load();
      await settle();
      assert.equal(
        store.getState().bootstrapError,
        NewConversationCopy.loadFailed,
      );
      assert.equal(store.getState().loadingPeople, false);
      assert.equal(canContinue(store.getState()), false);
      store.setInput("second");
      assert.notEqual(store.getState().bootstrapError, undefined);
      queries.options = { maximum: 10, people: [peer] };
      store.load();
      assert.equal(store.getState().bootstrapError, undefined);
      assert.equal(store.getState().loadingPeople, true);
      await settle();
      assert.equal(store.getState().bootstrapError, undefined);
      assert.equal(store.getState().maximumMembers, 10);
      assert.deepEqual(store.getState().people, [peer]);
      assert.equal(canContinue(store.getState()), true);
    });
  }
});

test("an unchanged input keeps its resolution", async () => {
  const queries = new FakeQueries();
  const { store } = makeStore(queries);
  store.setInput("peer");
  store.add();
  store.setInput("peer");
  await settle();
  assert.deepEqual(
    store.getState().recipients.map((person) => person.inboxId),
    ["inbox-peer"],
  );
  assert.equal(store.getState().input, "");
});

test("editing recipients does not cancel the initial people load", async () => {
  const peer: Person = { inboxId: "inbox-peer", handle: "person" };
  const queries = new FakeQueries({ people: [peer] });
  const gate = deferred();
  queries.peopleGate = gate.promise;
  const { store } = makeStore(queries);
  store.load();
  await settle();
  assert.equal(store.getState().loadingPeople, true);
  store.setGroup(true);
  store.setInput("person");
  store.toggle(peer);
  store.remove(peer.inboxId);
  gate.resolve();
  await settle();
  const state = store.getState();
  assert.equal(state.maximumMembers, 250);
  assert.equal(state.loadingPeople, false);
  assert.deepEqual(state.people, [peer]);
  assert.equal(state.input, "person");
  assert.equal(canContinue(state), true);
});

describe("editing the input or the people drops a resolution in flight", () => {
  const chosen: Person = { inboxId: "inbox-chosen", handle: "chosen" };
  const edits = {
    setInput: (store: NewConversationStore) => {
      store.setInput("peer, other");
    },
    toggle: (store: NewConversationStore) => {
      store.toggle(chosen);
    },
    remove: (store: NewConversationStore) => {
      store.remove(chosen.inboxId);
    },
    setGroup: (store: NewConversationStore) => {
      store.setGroup(true);
    },
    back: (store: NewConversationStore) => {
      store.back();
    },
  };
  for (const [name, edit] of Object.entries(edits)) {
    test(name, async () => {
      const queries = new FakeQueries();
      const { store } = makeStore(queries);
      if (name === "remove") store.toggle(chosen);
      const gate = deferred();
      queries.resolveGate = gate.promise;
      store.setInput("peer");
      store.add();
      assert.equal(store.getState().busy, true);
      await settle();
      edit(store);
      assert.equal(store.getState().busy, false);
      gate.resolve();
      await settle();
      const ids = store.getState().recipients.map((person) => person.inboxId);
      assert.equal(ids.includes("inbox-peer"), false);
      // A dropped run reports nothing; an error means it ran on.
      assert.equal(store.getState().error, undefined);
      assert.equal(
        store.getState().input,
        name === "setInput" ? "peer, other" : "peer",
      );
    });
  }
});

test("Add resolves every entry in order, skips duplicates, then clears the input", async () => {
  const queries = new FakeQueries();
  const { store } = makeStore(queries);
  store.load();
  await settle();
  store.setGroup(true);
  store.toggle({ inboxId: "inbox-lena", handle: "lena" });
  store.setInput("@jun,lena,,ava");
  store.add();
  await settle();
  assert.deepEqual(
    store.getState().recipients.map((person) => person.handle),
    ["lena", "jun", "ava"],
  );
  assert.equal(store.getState().input, "");
  assert.equal(store.getState().busy, false);
  assert.equal(store.getState().reviewing, false);
});

test("an entry that fails stops resolution, keeps the input and shows its copy", async () => {
  const queries = new FakeQueries();
  const { store } = makeStore(queries);
  store.load();
  await settle();
  store.setGroup(true);
  store.setInput("ava, bad, theo");
  store.add();
  await settle();
  assert.equal(store.getState().error, RecipientCopy.invalidHandle);
  assert.deepEqual(store.getState().recipients, []);
  assert.equal(store.getState().input, "ava, bad, theo");
  assert.ok(!queries.calls.some((call) => call.includes("theo")));
  store.setInput("boom");
  assert.equal(store.getState().error, undefined);
  store.add();
  await settle();
  assert.equal(store.getState().error, NewConversationCopy.failed);
});

test("count rules: none, a DM with two, a group over the limit", async () => {
  const queries = new FakeQueries({ maximum: 3 });
  const { store } = makeStore(queries);
  store.load();
  await settle();
  store.next();
  await settle();
  assert.equal(store.getState().error, NewConversationCopy.noRecipients);
  store.setInput("a, b");
  store.next();
  await settle();
  assert.equal(store.getState().error, NewConversationCopy.dmNeedsOne);
  assert.equal(store.getState().reviewing, false);
  store.setGroup(true);
  store.setInput("a, b, c");
  store.next();
  await settle();
  assert.equal(store.getState().error, NewConversationCopy.tooMany);
  store.setInput("a, b");
  store.next();
  await settle();
  assert.equal(store.getState().error, undefined);
  assert.equal(store.getState().reviewing, true);
  assert.equal(memberCount(store.getState()), 3);
  assert.deepEqual(
    queries.calls.filter((call) => call === "create"),
    [],
  );
});

test("Next shows the review; Back and the type picker leave it", async () => {
  const queries = new FakeQueries();
  const { store } = makeStore(queries);
  store.load();
  await settle();
  store.setInput("lena");
  store.next();
  await settle();
  assert.equal(store.getState().reviewing, true);
  store.back();
  assert.equal(store.getState().reviewing, false);
  store.next();
  await settle();
  assert.equal(store.getState().reviewing, true);
  store.setGroup(true);
  assert.equal(store.getState().reviewing, false);
});

test("Create makes a group with its name, description and permissions, then finishes", async () => {
  const queries = new FakeQueries();
  const { store, finished } = makeStore(queries);
  store.load();
  await settle();
  store.setGroup(true);
  store.toggle({ inboxId: "inbox-lena", handle: "lena" });
  store.toggle({ inboxId: "inbox-jun", handle: "jun" });
  store.toggle({ inboxId: "inbox-jun", handle: "jun" });
  store.setInput("ava");
  store.next();
  await settle();
  store.setName("Book club");
  store.setDescription("Monthly");
  store.setAdminOnly(true);
  store.create();
  await settle();
  assert.deepEqual(queries.created, [
    {
      inboxIds: ["inbox-lena", "inbox-ava"],
      group: true,
      name: "Book club",
      description: "Monthly",
      adminOnly: true,
    },
  ]);
  assert.deepEqual(finished, ["conversation-1"]);
  assert.equal(store.getState().busy, false);
});

test("a second Create while one runs creates no second conversation", async () => {
  const queries = new FakeQueries();
  const { store, finished } = makeStore(queries);
  store.load();
  await settle();
  store.setInput("theo");
  store.next();
  await settle();
  queries.during("create", () => {
    store.create();
  });
  store.create();
  await settle();
  assert.equal(queries.created.length, 1);
  assert.deepEqual(finished, ["conversation-1"]);
});

test("Create for a DM resolves the typed entry first", async () => {
  const queries = new FakeQueries();
  const { store, finished } = makeStore(queries);
  store.load();
  await settle();
  store.setInput("@theo");
  store.create();
  await settle();
  assert.deepEqual(
    queries.created.map((request) => request.inboxIds),
    [["inbox-theo"]],
  );
  assert.equal(queries.created[0]?.group, false);
  assert.deepEqual(finished, ["conversation-1"]);
});

test("a create failure keeps the sheet open with copy", async () => {
  const queries = new FakeQueries();
  queries.createFailure = new Error("network");
  const { store, finished } = makeStore(queries);
  store.load();
  await settle();
  store.setInput("theo");
  store.next();
  await settle();
  store.create();
  await settle();
  assert.deepEqual(finished, []);
  assert.equal(store.getState().error, NewConversationCopy.failed);
  assert.equal(store.getState().reviewing, true);
  assert.equal(store.getState().busy, false);
});

test("a session that ends during create does not finish", async () => {
  const queries = new FakeQueries();
  let valid = true;
  const { store, finished } = makeStore(queries, () => valid);
  store.load();
  await settle();
  store.setInput("theo");
  queries.during("create", () => {
    valid = false;
  });
  store.create();
  await settle();
  assert.equal(queries.created.length, 1);
  assert.deepEqual(finished, []);
});
