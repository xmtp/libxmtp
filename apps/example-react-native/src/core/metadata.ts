// Conversation metadata fields for the developer tools (ExampleCore/Metadata/
// MetadataRules.swift, OwnFieldDraft.swift, MetadataStore.swift and the
// mapping in ExampleSDK/MetadataQueries.swift).

import type {
  FieldValue,
  MetadataFieldDescriptor,
  MetadataPolicy,
  MetadataValue,
} from "@xmtp/react-native-sdk";

import { bytesToHex, hexToBytes } from "./hex.ts";
import { type GroupRole, POLICY_LABELS } from "./permissions.ts";
import { Store } from "./store.ts";

export const MetadataCopy = {
  updateFailed: "Cannot update these fields. Reload and try again.",
  absent: "Absent",
  ownEntry: "Own entry",
  unsupportedPolicy: (tag: number) => `Unsupported policy ${String(tag)}`,
} as const;

export type FieldShape =
  | "string"
  | "bytes"
  | "byteMap"
  | "byteSet"
  | "inboxSet"
  | "userString"
  | "userBytes"
  | "unsupported";

export interface FieldEntry {
  readonly key: string;
  readonly value: string;
}

export interface MetadataField {
  readonly id: number;
  readonly name: string;
  readonly shape: FieldShape;
  /** "Insert: …; Update: …; Delete: …". */
  readonly policy: string;
  readonly userField: boolean;
  /** Every policy is one the app understands. */
  readonly editable: boolean;
  readonly canInsert: boolean;
  readonly canUpdate: boolean;
  readonly canDelete: boolean;
  /** This entry (own entry for user fields) has a value. */
  readonly present: boolean;
  /** The component has any value (immutable fields can be set once). */
  readonly componentPresent: boolean;
  /** Scalar text: the string, or lowercase hex for bytes. */
  readonly scalar: string;
  readonly entries: readonly FieldEntry[];
}

export interface MemberMetadata {
  readonly inboxId: string;
  readonly fields: readonly MetadataField[];
}

export interface MetadataSnapshot {
  readonly fields: readonly MetadataField[];
  readonly members: readonly MemberMetadata[];
  /** Names of server components offered here but not registered. */
  readonly missing: readonly string[];
}

export type MetadataRuleProblem =
  | "hex"
  | "inbox"
  | "valueLimit"
  | "collectionLimit"
  | "unsupported"
  | "changed"
  | "immutable"
  | "entryChanged";

/** A metadata input or write the rules reject. */
export class MetadataRuleError extends Error {
  readonly problem: MetadataRuleProblem;

  constructor(problem: MetadataRuleProblem) {
    super(`Metadata rule: ${problem}`);
    this.name = "MetadataRuleError";
    this.problem = problem;
  }
}

export type FieldEntryAction = "insert" | "update" | "delete";

/** IDs 0xFD00–0xFEFF can be set once and never changed. */
export function isImmutable(field: Pick<MetadataField, "id">): boolean {
  return field.id >= 0xfd00 && field.id <= 0xfeff;
}

/** The field can be written at all by this member. */
export function canWrite(field: MetadataField): boolean {
  return (
    field.editable &&
    (field.canInsert || field.canUpdate || field.canDelete) &&
    field.shape !== "unsupported" &&
    (!isImmutable(field) || !field.componentPresent)
  );
}

export function allows(
  field: MetadataField,
  action: FieldEntryAction,
): boolean {
  if (!canWrite(field)) return false;
  switch (action) {
    case "insert":
      return field.canInsert;
    case "update":
      return field.canUpdate;
    case "delete":
      return field.canDelete;
  }
}

/** Set inserts when absent and updates when present. */
export function canSet(field: MetadataField): boolean {
  return allows(field, field.present ? "update" : "insert");
}

const HEX = /^[0-9a-fA-F]*$/u;
const INBOX = /^[0-9a-f]{64}$/u;
const MAX_HEX_CHARACTERS = 16384;
const MAX_STRING_BYTES = 8192;
const MAX_COLLECTION_BYTES = 65536;

const utf8Length = (text: string): number =>
  new TextEncoder().encode(text).length;

/** Bytes from even-length hex of at most 16384 characters. */
export function hexBytes(text: string): Uint8Array {
  if (text.length % 2 !== 0 || !HEX.test(text)) {
    throw new MetadataRuleError("hex");
  }
  if (text.length > MAX_HEX_CHARACTERS) {
    throw new MetadataRuleError("valueLimit");
  }
  const bytes = hexToBytes(text);
  if (bytes === undefined) throw new MetadataRuleError("hex");
  return bytes;
}

/** An inbox ID key: exactly 64 lowercase hex characters. */
function inboxKey(text: string): string {
  if (!INBOX.test(text)) throw new MetadataRuleError("inbox");
  return text;
}

/** The canonical scalar text: the string (≤ 8192 UTF-8 bytes) or lowercase hex. */
export function scalarText(text: string, shape: FieldShape): string {
  switch (shape) {
    case "string":
    case "userString":
      if (utf8Length(text) > MAX_STRING_BYTES) {
        throw new MetadataRuleError("valueLimit");
      }
      return text;
    case "bytes":
    case "byteMap":
    case "userBytes":
      return bytesToHex(hexBytes(text));
    default:
      throw new MetadataRuleError("unsupported");
  }
}

/** The canonical collection key: lowercase hex, or an inbox ID. */
export function canonicalKey(text: string, shape: FieldShape): string {
  switch (shape) {
    case "byteMap":
    case "byteSet":
      return scalarText(text, "bytes");
    case "inboxSet":
    case "userString":
    case "userBytes":
      return inboxKey(text);
    default:
      throw new MetadataRuleError("unsupported");
  }
}

/** Update for an existing byte map key, else insert. */
export function entryAction(
  text: string,
  field: MetadataField,
): FieldEntryAction {
  const key = canonicalKey(text, field.shape);
  return field.shape === "byteMap" &&
    field.entries.some((entry) => entry.key === key)
    ? "update"
    : "insert";
}

/** The length prefix of an encoded value: 1, 2 or 4 bytes. */
function prefix(count: number): number {
  return count < 64 ? 1 : count < 16384 ? 2 : 4;
}

/** Rejects a collection whose serialized payload exceeds 65536 bytes. */
export function checkCollection(
  entries: readonly FieldEntry[],
  shape: FieldShape,
): void {
  let payload = 0;
  for (const entry of entries) {
    if (
      shape === "inboxSet" ||
      shape === "userString" ||
      shape === "userBytes"
    ) {
      inboxKey(entry.key);
      payload += 33;
    } else {
      const count = hexBytes(entry.key).length;
      payload += count + prefix(count);
    }
    if (shape === "byteMap" || shape === "userBytes") {
      const count = hexBytes(entry.value).length;
      payload += count + prefix(count);
    }
    if (shape === "userString") {
      const count = utf8Length(entry.value);
      if (count > MAX_STRING_BYTES) throw new MetadataRuleError("valueLimit");
      payload += count + prefix(count);
    }
    if (payload > MAX_COLLECTION_BYTES) {
      throw new MetadataRuleError("collectionLimit");
    }
  }
  if (payload + prefix(payload) > MAX_COLLECTION_BYTES) {
    throw new MetadataRuleError("collectionLimit");
  }
}

/** A field may be written only if its definition did not change since it was shown. */
export function checkUnchanged(
  before: MetadataField,
  now: MetadataField,
): void {
  if (
    before.id !== now.id ||
    before.shape !== now.shape ||
    before.policy !== now.policy ||
    before.userField !== now.userField ||
    before.editable !== now.editable ||
    before.canInsert !== now.canInsert ||
    before.canUpdate !== now.canUpdate ||
    before.canDelete !== now.canDelete
  ) {
    throw new MetadataRuleError("changed");
  }
  if (!canWrite(now)) throw new MetadataRuleError("immutable");
}

/**
 * The registry read just before a write must still define the field the
 * loaded snapshot shows (shape, policy, editable, user field).
 */
export function checkLatest(
  now: MetadataField,
  descriptor: MetadataFieldDescriptor,
): void {
  const latest = metadataField(descriptor, undefined);
  if (
    latest.shape !== now.shape ||
    latest.policy !== now.policy ||
    latest.editable !== now.editable ||
    latest.userField !== now.userField
  ) {
    throw new MetadataRuleError("changed");
  }
}

/** `"<name> · 0xC101 · string"`. */
export function fieldHeader(field: MetadataField): string {
  const id = field.id.toString(16).toUpperCase().padStart(4, "0");
  return `${field.name} · 0x${id} · ${field.shape}`;
}

/** Read-only text: the scalar, "key: value" lines, or "Absent". */
export function readOnlyValue(field: MetadataField): string {
  if (!field.present) return MetadataCopy.absent;
  if (field.entries.length === 0) return field.scalar;
  return field.entries
    .map((entry) => entry.key + (entry.value === "" ? "" : `: ${entry.value}`))
    .join("\n");
}

// SDK mapping (`metadataShape`, `metadataPolicy`, `metadataAllows`).

function metadataShape(descriptor: MetadataFieldDescriptor): FieldShape {
  const type = descriptor.componentType;
  const user = descriptor.isUserField;
  switch (type.kind) {
    case "string":
      return user ? "unsupported" : "string";
    case "bytes":
      return user ? "unsupported" : "bytes";
    case "map":
      if (type.keyType === "inboxId" && user) {
        return type.valueType === "string" ? "userString" : "userBytes";
      }
      return type.keyType === "bytes" && type.valueType === "bytes" && !user
        ? "byteMap"
        : "unsupported";
    case "set":
      if (user) return "unsupported";
      return type.keyType === "bytes" ? "byteSet" : "inboxSet";
    case "unknown":
      return "unsupported";
  }
}

/** A policy label and whether the app understands it. */
function policyLabel(policy: MetadataPolicy): {
  readonly label: string;
  readonly known: boolean;
} {
  if (policy.kind === "base") {
    switch (policy.value.kind) {
      case "allow":
        return { label: POLICY_LABELS.allow, known: true };
      case "deny":
        return { label: POLICY_LABELS.deny, known: true };
      case "allowIfAdmin":
        return { label: POLICY_LABELS.admin, known: true };
      case "allowIfSuperAdmin":
        return { label: POLICY_LABELS.superAdmin, known: true };
      case "allowIfSelfOrNonMember":
        return { label: MetadataCopy.ownEntry, known: true };
      case "unknown":
        return {
          label: MetadataCopy.unsupportedPolicy(policy.value.tag),
          known: false,
        };
    }
  }
  const children = policy.value.map(policyLabel);
  const separator = policy.kind === "and" ? " and " : " or ";
  return {
    label: `(${children.map((child) => child.label).join(separator)})`,
    known: children.length > 0 && children.every((child) => child.known),
  };
}

/** Whether `role` passes a policy (`ownEntry` for a user's own entry). */
function policyAllows(
  policy: MetadataPolicy,
  role: GroupRole,
  ownEntry: boolean,
): boolean {
  if (policy.kind === "base") {
    switch (policy.value.kind) {
      case "allow":
        return true;
      case "deny":
      case "unknown":
        return false;
      case "allowIfAdmin":
        return role === "admin" || role === "superAdmin";
      case "allowIfSuperAdmin":
        return role === "superAdmin";
      case "allowIfSelfOrNonMember":
        return ownEntry;
    }
  }
  if (policy.kind === "and") {
    return (
      policy.value.length > 0 &&
      policy.value.every((child) => policyAllows(child, role, ownEntry))
    );
  }
  return policy.value.some((child) => policyAllows(child, role, ownEntry));
}

/** Scalar text of an SDK value: the string, or lowercase hex. */
export function fieldValueText(value: FieldValue): string {
  return value.kind === "string" ? value.value : bytesToHex(value.value);
}

/** A field from its descriptor and (non-user) value. */
export function metadataField(
  descriptor: MetadataFieldDescriptor,
  value: MetadataValue | undefined,
  role: GroupRole = "member",
): MetadataField {
  const { insert, update, delete: remove } = descriptor.permissions;
  const labels = [insert, update, remove].map(policyLabel);
  const user = descriptor.isUserField;
  const entries: FieldEntry[] =
    value?.kind === "map"
      ? value.value.map((entry) => ({
          key:
            entry.key.kind === "inboxId"
              ? entry.key.value
              : bytesToHex(entry.key.value),
          value: fieldValueText(entry.value),
        }))
      : value?.kind === "set"
        ? value.value.map((key) => ({
            key: key.kind === "inboxId" ? key.value : bytesToHex(key.value),
            value: "",
          }))
        : [];
  return {
    id: descriptor.field.componentId,
    name:
      descriptor.field.name ??
      `Component ${String(descriptor.field.componentId)}`,
    shape: metadataShape(descriptor),
    policy: ["Insert", "Update", "Delete"]
      .map((title, index) => `${title}: ${labels[index]?.label ?? ""}`)
      .join("; "),
    userField: user,
    editable: labels.every((label) => label.known),
    canInsert: policyAllows(insert, role, user),
    canUpdate: policyAllows(update, role, user),
    canDelete: policyAllows(remove, role, user),
    present: value !== undefined,
    componentPresent: value !== undefined,
    scalar: value?.kind === "scalar" ? fieldValueText(value.value) : "",
    entries,
  };
}

/** The descriptors the screens list: user fields and IDs 0xC000–0xFEFF. */
export function isListedField(descriptor: MetadataFieldDescriptor): boolean {
  const id = descriptor.field.componentId;
  return descriptor.isUserField || (id >= 0xc000 && id <= 0xfeff);
}

// Own field draft (`OwnFieldDraft`): absent differs from "".

export type OwnFieldValue =
  | { readonly kind: "absent" }
  | { readonly kind: "value"; readonly text: string };

export const ABSENT: OwnFieldValue = { kind: "absent" };

const sameValue = (
  a: OwnFieldValue | undefined,
  b: OwnFieldValue | undefined,
) =>
  a?.kind === b?.kind &&
  (a?.kind !== "value" || b?.kind !== "value" || a.text === b.text);

export interface OwnFieldDraft {
  readonly baseline: ReadonlyMap<number, OwnFieldValue>;
  readonly values: ReadonlyMap<number, OwnFieldValue>;
}

export const emptyDraft: OwnFieldDraft = {
  baseline: new Map(),
  values: new Map(),
};

export function draftValue(draft: OwnFieldDraft, id: number): OwnFieldValue {
  return draft.values.get(id) ?? ABSENT;
}

export function draftChanged(draft: OwnFieldDraft, id: number): boolean {
  return !sameValue(draft.values.get(id), draft.baseline.get(id));
}

export function setDraft(
  draft: OwnFieldDraft,
  id: number,
  value: OwnFieldValue,
): OwnFieldDraft {
  return { ...draft, values: new Map(draft.values).set(id, value) };
}

/**
 * Takes committed values for writable own fields, keeping an unsaved edit
 * that still differs from both its old baseline and the new committed value.
 */
export function mergeDraft(
  draft: OwnFieldDraft,
  fields: readonly MetadataField[],
): OwnFieldDraft {
  const baseline = new Map<number, OwnFieldValue>();
  const values = new Map<number, OwnFieldValue>();
  for (const field of fields) {
    if (!field.userField || !canWrite(field)) continue;
    const committed: OwnFieldValue = field.present
      ? { kind: "value", text: field.scalar }
      : ABSENT;
    const old = draft.baseline.get(field.id);
    const edited = draft.values.get(field.id);
    if (
      old !== undefined &&
      edited !== undefined &&
      !sameValue(edited, old) &&
      !sameValue(edited, committed)
    ) {
      baseline.set(field.id, old);
      values.set(field.id, edited);
    } else {
      baseline.set(field.id, committed);
      values.set(field.id, committed);
    }
  }
  return { baseline, values };
}

/** Changed own fields: text to set, or undefined to clear. */
export function draftChanges(
  draft: OwnFieldDraft,
): ReadonlyMap<number, string | undefined> {
  const changes = new Map<number, string | undefined>();
  for (const [id, value] of draft.values) {
    if (sameValue(value, draft.baseline.get(id))) continue;
    changes.set(id, value.kind === "value" ? value.text : undefined);
  }
  return changes;
}

// Store (`MetadataStore`).

export type MetadataEdit =
  | {
      readonly kind: "scalar";
      readonly id: number;
      readonly value: string | undefined;
    }
  | {
      readonly kind: "entry";
      readonly id: number;
      readonly action: FieldEntryAction;
      readonly key: string;
      readonly value: string;
    }
  | {
      readonly kind: "own";
      readonly changes: ReadonlyMap<number, string | undefined>;
    };

export interface MetadataQueries {
  load(): Promise<MetadataSnapshot>;
  edit(edit: MetadataEdit, baseline: MetadataSnapshot): Promise<void>;
}

export interface MetadataState {
  readonly snapshot: MetadataSnapshot;
  readonly busy: boolean;
  readonly error?: string;
}

const emptySnapshot: MetadataSnapshot = {
  fields: [],
  members: [],
  missing: [],
};

export interface MetadataStoreOptions {
  readonly admit?: () => boolean;
  readonly run?: (job: () => Promise<void>) => void;
}

/**
 * Loads fields and writes edits one at a time. An edit reloads after it;
 * a failure reloads too and shows "Cannot update these fields…".
 */
export class MetadataStore extends Store<MetadataState> {
  readonly #queries: MetadataQueries;
  readonly #admit: () => boolean;
  readonly #run: (job: () => Promise<void>) => void;
  #disposed = false;

  constructor(queries: MetadataQueries, options: MetadataStoreOptions = {}) {
    super({ snapshot: emptySnapshot, busy: false });
    this.#queries = queries;
    this.#admit = options.admit ?? (() => true);
    this.#run =
      options.run ??
      ((job) => {
        void job();
      });
  }

  get disposed(): boolean {
    return this.#disposed;
  }

  dispose(): void {
    this.#disposed = true;
  }

  #admits(): boolean {
    return !this.#disposed && this.#admit();
  }

  #apply(change: (state: MetadataState) => MetadataState): void {
    if (this.#admits()) this.setState(change);
  }

  refresh(): void {
    this.#send(undefined);
  }

  edit(edit: MetadataEdit): void {
    this.#send(edit);
  }

  #send(edit: MetadataEdit | undefined): void {
    if (this.getState().busy || !this.#admits()) return;
    const baseline = this.getState().snapshot;
    this.setState((state) => ({ snapshot: state.snapshot, busy: true }));
    this.#run(async () => {
      try {
        if (edit !== undefined) await this.#queries.edit(edit, baseline);
        const snapshot = await this.#queries.load();
        this.#apply((state) => ({ ...state, snapshot }));
      } catch {
        try {
          const snapshot = await this.#queries.load();
          this.#apply((state) => ({ ...state, snapshot }));
        } catch {
          // The error below is what the screen shows.
        }
        this.#apply((state) => ({
          ...state,
          error: MetadataCopy.updateFailed,
        }));
      }
      this.#apply((state) => ({ ...state, busy: false }));
    });
  }
}
