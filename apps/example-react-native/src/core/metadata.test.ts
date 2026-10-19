import assert from "node:assert/strict";
import { test } from "node:test";

import type {
  MetadataFieldDescriptor,
  MetadataPolicy,
} from "@xmtp/react-native-sdk";

import {
  ABSENT,
  canWrite,
  checkCollection,
  checkLatest,
  checkUnchanged,
  draftChanges,
  emptyDraft,
  entryAction,
  fieldHeader,
  hexBytes,
  MetadataCopy,
  type MetadataField,
  metadataField,
  MetadataRuleError,
  type MetadataSnapshot,
  MetadataStore,
  mergeDraft,
  scalarText,
  setDraft,
} from "./metadata.ts";
import { POLICY_LABELS } from "./permissions.ts";
import { settle } from "./testing.ts";

function field(overrides: Partial<MetadataField>): MetadataField {
  return {
    id: 0xc001,
    name: "topic",
    shape: "string",
    policy: "",
    userField: false,
    editable: true,
    canInsert: true,
    canUpdate: true,
    canDelete: true,
    present: false,
    componentPresent: false,
    scalar: "",
    entries: [],
    ...overrides,
  };
}

function rejects(problem: string, run: () => unknown): void {
  assert.throws(
    run,
    (error: unknown) =>
      error instanceof MetadataRuleError && error.problem === problem,
  );
}

test("bytes are even hex of at most 16384 characters", () => {
  assert.equal(hexBytes("ab".repeat(8192)).length, 8192);
  rejects("valueLimit", () => hexBytes("ab".repeat(8193)));
  rejects("hex", () => hexBytes("xyz"));
  rejects("hex", () => hexBytes("abc"));
  assert.equal(scalarText("ABCD", "bytes"), "abcd");
});

test("strings are at most 8192 UTF-8 bytes", () => {
  assert.equal(scalarText("é".repeat(4096), "string").length, 4096);
  rejects("valueLimit", () => scalarText("é".repeat(4097), "string"));
  rejects("unsupported", () => scalarText("x", "unsupported"));
});

test("the outer prefix decides a payload at the limit", () => {
  const full = Array.from({ length: 7 }, (_, index) => ({
    key: index.toString(16).padStart(2, "0"),
    value: "ff".repeat(8192),
  }));
  // 7 x 8196 + (2 + 8156 + 2) = 65532 bytes: with its 4 byte prefix, 65536.
  checkCollection(
    [...full, { key: "07", value: "ff".repeat(8156) }],
    "byteMap",
  );
  // 65536 bytes fits the entry check but not the outer prefix.
  rejects("collectionLimit", () => {
    checkCollection(
      [...full, { key: "07", value: "ff".repeat(8160) }],
      "byteMap",
    );
  });
});

test("a changed policy or an immutable field blocks writes", () => {
  const old = field({ policy: "Admins" });
  rejects("changed", () => {
    checkUnchanged(old, { ...old, policy: "Denied" });
  });
  const immutable = field({
    id: 0xfd00,
    shape: "inboxSet",
    componentPresent: true,
  });
  assert.equal(canWrite(immutable), false);
  rejects("immutable", () => {
    checkUnchanged(immutable, immutable);
  });
  assert.equal(canWrite({ ...immutable, componentPresent: false }), true);
});

test("an uppercase byte map key updates the existing entry", () => {
  const map = field({
    shape: "byteMap",
    entries: [{ key: "abcd", value: "01" }],
  });
  assert.equal(entryAction("ABCD", map), "update");
  assert.equal(entryAction("ab", map), "insert");
  assert.equal(entryAction("abcd", { ...map, shape: "byteSet" }), "insert");
});

test("the own draft writes only intentional changes", () => {
  const absent = field({
    id: 0xc201,
    name: "status",
    shape: "userString",
    userField: true,
  });
  const present = field({
    id: 0xc202,
    name: "device",
    shape: "userString",
    userField: true,
    present: true,
    scalar: "Phone",
  });
  let draft = mergeDraft(emptyDraft, [absent, present]);
  assert.equal(draftChanges(draft).size, 0);
  draft = setDraft(draft, absent.id, { kind: "value", text: "" });
  assert.deepEqual([...draftChanges(draft)], [[absent.id, ""]]);
  draft = setDraft(draft, present.id, ABSENT);
  assert.equal(draftChanges(draft).size, 2);
  assert.equal(draftChanges(draft).get(present.id), undefined);
  assert.equal(draftChanges(draft).has(present.id), true);
  // A reload with the same committed values keeps unsaved edits.
  draft = mergeDraft(draft, [absent, present]);
  assert.equal(draftChanges(draft).size, 2);
  // Once the group commits them, nothing is left to save.
  draft = mergeDraft(draft, [
    { ...absent, present: true },
    { ...present, present: false, scalar: "" },
  ]);
  assert.equal(draftChanges(draft).size, 0);
});

const base = (kind: string): MetadataPolicy =>
  ({ kind: "base", value: { kind } }) as MetadataPolicy;

test("a descriptor maps to a field with its header, policies, authority and value", () => {
  const descriptor: MetadataFieldDescriptor = {
    field: { componentId: 0xc101, name: "Topic" },
    componentType: { kind: "string" },
    permissions: {
      insert: base("allowIfAdmin"),
      update: base("allowIfAdmin"),
      delete: base("deny"),
    },
    isUserField: false,
  };
  const mapped = metadataField(
    descriptor,
    { kind: "scalar", value: { kind: "string", value: "MLS" } },
    "admin",
  );
  assert.equal(fieldHeader(mapped), "Topic · 0xC101 · string");
  assert.equal(
    mapped.policy,
    `Insert: ${POLICY_LABELS.admin}; Update: ${POLICY_LABELS.admin}; Delete: ${POLICY_LABELS.deny}`,
  );
  assert.equal(mapped.scalar, "MLS");
  assert.equal(mapped.editable, true);
  assert.deepEqual(
    (["member", "admin", "superAdmin"] as const).map(
      (role) => metadataField(descriptor, undefined, role).canUpdate,
    ),
    [false, true, true],
  );
  assert.equal(mapped.canDelete, false);
  const both: MetadataPolicy = {
    kind: "and",
    value: [base("allowIfAdmin"), base("allowIfSelfOrNonMember")],
  };
  const unknown: MetadataPolicy = {
    kind: "any",
    value: [{ kind: "base", value: { kind: "unknown", tag: 9 } }],
  };
  const own: MetadataFieldDescriptor = {
    field: { componentId: 0xc201 },
    componentType: { kind: "map", keyType: "inboxId", valueType: "bytes" },
    permissions: { insert: both, update: both, delete: unknown },
    isUserField: true,
  };
  const user = metadataField(own, undefined, "admin");
  assert.equal(user.name, `Component ${String(0xc201)}`);
  assert.equal(user.shape, "userBytes");
  assert.equal(user.present, false);
  assert.equal(
    user.policy,
    `Insert: (${POLICY_LABELS.admin} and ${MetadataCopy.ownEntry}); Update: (${POLICY_LABELS.admin} and ${MetadataCopy.ownEntry}); Delete: (${MetadataCopy.unsupportedPolicy(9)})`,
  );
  // The unsupported label names its tag.
  assert.ok(user.policy.split("; ").at(-1)?.includes("9"));
  assert.equal(user.editable, false);
  assert.equal(user.canInsert, true);
  assert.equal(user.canDelete, false);
  // The own-entry rule needs a user field, and an empty "and" allows no one.
  assert.equal(
    metadataField({ ...own, isUserField: false }, undefined, "admin").canInsert,
    false,
  );
  const empty: MetadataPolicy = { kind: "and", value: [] };
  assert.equal(
    metadataField(
      { ...own, permissions: { ...own.permissions, insert: empty } },
      undefined,
      "superAdmin",
    ).canInsert,
    false,
  );
});

test("a registry change after the load blocks the write", () => {
  const descriptor: MetadataFieldDescriptor = {
    field: { componentId: 0xc102, name: "Topic" },
    componentType: { kind: "string" },
    permissions: {
      insert: base("allowIfAdmin"),
      update: base("allowIfAdmin"),
      delete: base("deny"),
    },
    isUserField: false,
  };
  const now = metadataField(
    descriptor,
    { kind: "scalar", value: { kind: "string", value: "MLS" } },
    "admin",
  );
  checkLatest(now, descriptor);
  const changes: MetadataFieldDescriptor[] = [
    {
      ...descriptor,
      permissions: { ...descriptor.permissions, update: base("deny") },
    },
    { ...descriptor, componentType: { kind: "bytes" } },
    { ...descriptor, isUserField: true },
  ];
  for (const latest of changes) {
    rejects("changed", () => {
      checkLatest(now, latest);
    });
  }
});

test("a failed edit reloads and shows the update error until the next action", async () => {
  const snapshot: MetadataSnapshot = {
    fields: [field({})],
    members: [],
    missing: [],
  };
  let loads = 0;
  const store = new MetadataStore({
    load: () => {
      loads += 1;
      return Promise.resolve(snapshot);
    },
    edit: () => Promise.reject(new Error("denied")),
  });
  store.edit({ kind: "scalar", id: 0xc001, value: "x" });
  assert.equal(store.getState().busy, true);
  store.refresh();
  await settle();
  assert.equal(store.getState().busy, false);
  assert.equal(loads, 1);
  assert.deepEqual(store.getState().snapshot, snapshot);
  assert.equal(store.getState().error, MetadataCopy.updateFailed);
  store.refresh();
  await settle();
  assert.equal(store.getState().error, undefined);
});
