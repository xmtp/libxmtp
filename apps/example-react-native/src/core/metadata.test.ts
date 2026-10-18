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
  checkUnchanged,
  draftChanges,
  emptyDraft,
  entryAction,
  fieldHeader,
  hexBytes,
  type MetadataField,
  metadataField,
  MetadataRuleError,
  type MetadataSnapshot,
  MetadataStore,
  mergeDraft,
  policyAllows,
  policyLabel,
  scalarText,
  setDraft,
} from "./metadata.ts";

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

test("a serialized collection includes its outer prefix", () => {
  const entries = Array.from({ length: 7 }, (_, index) => ({
    key: index.toString(16).padStart(2, "0"),
    value: "ff".repeat(8192),
  }));
  checkCollection(entries, "byteMap");
  rejects("collectionLimit", () => {
    checkCollection(
      [...entries, { key: "08", value: "ff".repeat(8192) }],
      "byteMap",
    );
  });
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

test("policy labels and authority follow the role", () => {
  assert.equal(policyLabel(base("allowIfAdmin")).label, "Admins");
  const both: MetadataPolicy = {
    kind: "and",
    value: [base("allowIfAdmin"), base("allowIfSelfOrNonMember")],
  };
  assert.equal(policyLabel(both).label, "(Admins and Own entry)");
  const unknown: MetadataPolicy = {
    kind: "base",
    value: { kind: "unknown", tag: 9 },
  };
  assert.deepEqual(policyLabel({ kind: "any", value: [unknown] }), {
    label: "(Unsupported policy 9)",
    known: false,
  });
  assert.equal(policyAllows(base("allowIfAdmin"), "member", false), false);
  assert.equal(policyAllows(base("allowIfAdmin"), "superAdmin", false), true);
  assert.equal(policyAllows(both, "admin", false), false);
  assert.equal(policyAllows(both, "admin", true), true);
  assert.equal(
    policyAllows({ kind: "and", value: [] }, "superAdmin", true),
    false,
  );
});

test("a descriptor maps to a field with its header, shape and value", () => {
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
  assert.equal(mapped.policy, "Insert: Admins; Update: Admins; Delete: Denied");
  assert.equal(mapped.scalar, "MLS");
  assert.equal(mapped.canUpdate, true);
  assert.equal(mapped.canDelete, false);
  const user = metadataField(
    {
      ...descriptor,
      field: { componentId: 0xc201 },
      componentType: { kind: "map", keyType: "inboxId", valueType: "bytes" },
      isUserField: true,
    },
    undefined,
  );
  assert.equal(user.name, `Component ${String(0xc201)}`);
  assert.equal(user.shape, "userBytes");
  assert.equal(user.present, false);
});

test("a failed edit reloads and shows the update error", async () => {
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
  for (let index = 0; index < 50 && store.getState().busy; index += 1) {
    await new Promise((resolve) => setImmediate(resolve));
  }
  assert.equal(loads, 1);
  assert.equal(store.getState().snapshot, snapshot);
  assert.equal(
    store.getState().error,
    "Cannot update these fields. Reload and try again.",
  );
});
