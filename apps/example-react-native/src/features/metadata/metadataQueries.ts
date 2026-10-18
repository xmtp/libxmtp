// The SDK side of `MetadataStore` (ExampleSDK/MetadataQueries.swift).

import {
  type ComponentMutation,
  type Conversation,
  type FieldKey,
  type FieldValue,
  Group,
  type MetadataFieldRef,
  type UserFieldUpdate,
} from "@xmtp/react-native-sdk";

import { bytesToHex } from "@/core/hex.ts";
import {
  allows,
  canonicalKey,
  canSet,
  checkCollection,
  checkLatest,
  checkUnchanged,
  type FieldEntry,
  type FieldEntryAction,
  type FieldShape,
  fieldValueText,
  hexBytes,
  isImmutable,
  isListedField,
  type MetadataEdit,
  type MetadataField,
  metadataField,
  type MetadataQueries,
  MetadataRuleError,
  type MetadataSnapshot,
  scalarText,
} from "@/core/metadata.ts";
import type { GroupRole } from "@/core/permissions.ts";
import {
  ConversationUnavailableError,
  StaleWorkError,
} from "@/features/details/groupQueries";
import type { ActiveSession } from "@/session";

function fieldValue(text: string, shape: FieldShape): FieldValue {
  const canonical = scalarText(text, shape);
  return shape === "string" || shape === "userString"
    ? { kind: "string", value: canonical }
    : { kind: "bytes", value: hexBytes(canonical) };
}

function fieldKey(text: string, shape: FieldShape): FieldKey {
  const canonical = canonicalKey(text, shape);
  return shape === "inboxSet"
    ? { kind: "inboxId", value: canonical }
    : { kind: "bytes", value: hexBytes(canonical) };
}

const keyText = (key: FieldKey): string =>
  key.kind === "inboxId" ? key.value : bytesToHex(key.value);

export class SdkMetadataQueries implements MetadataQueries {
  readonly #session: ActiveSession;
  readonly #id: string;
  readonly #valid: () => boolean;

  constructor(session: ActiveSession, id: string, valid: () => boolean) {
    this.#session = session;
    this.#id = id;
    this.#valid = valid;
  }

  #check(): void {
    if (!this.#valid()) throw new StaleWorkError();
  }

  async #conversation(): Promise<Conversation> {
    const value = await this.#session.client.conversations.getById(this.#id);
    if (value === undefined) throw new ConversationUnavailableError();
    return value;
  }

  async #ownRole(conversation: Conversation): Promise<GroupRole> {
    const own = (await conversation.members()).find(
      (member) => member.inboxId === this.#session.inboxId,
    );
    return own?.permissionLevel ?? "member";
  }

  async load(): Promise<MetadataSnapshot> {
    this.#check();
    const own = this.#session.inboxId;
    const conversation = await this.#conversation();
    const descriptors = await conversation.metadataFields();
    const role = await this.#ownRole(conversation);
    const users = descriptors.filter(
      (descriptor) =>
        descriptor.isUserField &&
        metadataField(descriptor, undefined).shape !== "unsupported",
    );
    const profiles =
      users.length === 0
        ? new Map<string, never[]>()
        : await conversation.userData(
            users.map((descriptor) => descriptor.field),
            undefined,
          );
    const fields: MetadataField[] = [];
    for (const descriptor of descriptors.filter(isListedField)) {
      let field = metadataField(descriptor, undefined, role);
      if (field.shape !== "unsupported") {
        if (descriptor.isUserField) {
          const value = (profiles.get(own) ?? []).find(
            (entry) => entry.field.componentId === descriptor.field.componentId,
          )?.value;
          field = {
            ...field,
            present: value !== undefined,
            componentPresent: isImmutable(field)
              ? (await conversation.metadataValue(descriptor.field)) !==
                undefined
              : value !== undefined,
            scalar: value === undefined ? "" : fieldValueText(value),
          };
        } else {
          field = metadataField(
            descriptor,
            await conversation.metadataValue(descriptor.field),
            role,
          );
        }
      }
      fields.push(field);
    }
    const members = [...profiles]
      .filter(([inboxId]) => inboxId !== own)
      .map(([inboxId, values]) => ({
        inboxId,
        fields: users.map((descriptor) => {
          const value = values.find(
            (entry) => entry.field.componentId === descriptor.field.componentId,
          )?.value;
          return {
            ...metadataField(descriptor, undefined),
            present: value !== undefined,
            scalar: value === undefined ? "" : fieldValueText(value),
            editable: false,
          };
        }),
      }));
    const group = conversation instanceof Group;
    const ids = new Set(
      descriptors.map((descriptor) => descriptor.field.componentId),
    );
    const missing = (
      this.#session.serverConfiguration()?.applicationComponents ?? []
    )
      .filter(
        (component) =>
          (group ? component.inGroups : component.inDms) &&
          !ids.has(component.componentId),
      )
      .map((component) => component.name);
    this.#check();
    return {
      fields: fields.sort((a, b) => a.id - b.id),
      members,
      missing,
    };
  }

  async edit(edit: MetadataEdit, baseline: MetadataSnapshot): Promise<void> {
    this.#check();
    const current = await this.load();
    const conversation = await this.#conversation();
    const descriptors = await conversation.metadataFields();
    const field = (id: number): [MetadataField, MetadataFieldRef] => {
      const old = baseline.fields.find((entry) => entry.id === id);
      const now = current.fields.find((entry) => entry.id === id);
      const descriptor = descriptors.find(
        (entry) => entry.field.componentId === id,
      );
      if (old === undefined || now === undefined || descriptor === undefined) {
        throw new MetadataRuleError("changed");
      }
      checkUnchanged(old, now);
      checkLatest(now, descriptor);
      return [now, descriptor.field];
    };
    switch (edit.kind) {
      case "scalar":
        await this.#editScalar(edit.value, field(edit.id), conversation);
        return;
      case "entry":
        await this.#editEntry(
          edit.action,
          edit.key,
          edit.value,
          field(edit.id),
          conversation,
        );
        return;
      case "own":
        await this.#editOwn(edit.changes, field, conversation);
        return;
    }
  }

  async #editScalar(
    text: string | undefined,
    [field, ref]: [MetadataField, MetadataFieldRef],
    conversation: Conversation,
  ): Promise<void> {
    if (
      field.userField ||
      (field.shape !== "string" && field.shape !== "bytes")
    ) {
      throw new MetadataRuleError("unsupported");
    }
    if (isImmutable(field) && text === undefined) {
      throw new MetadataRuleError("immutable");
    }
    if (text === undefined ? !allows(field, "delete") : !canSet(field)) {
      throw new MetadataRuleError("immutable");
    }
    const operation: ComponentMutation =
      text === undefined
        ? { kind: "remove" }
        : { kind: "replace", value: fieldValue(text, field.shape) };
    this.#check();
    await conversation.updateMetadataField(ref, operation);
  }

  async #editEntry(
    action: FieldEntryAction,
    text: string,
    content: string,
    [field, ref]: [MetadataField, MetadataFieldRef],
    conversation: Conversation,
  ): Promise<void> {
    if (!allows(field, action)) throw new MetadataRuleError("immutable");
    if (field.userField) throw new MetadataRuleError("unsupported");
    if (isImmutable(field) && action !== "insert") {
      throw new MetadataRuleError("immutable");
    }
    const key = canonicalKey(text, field.shape);
    const exists = field.entries.some((entry) => entry.key === key);
    if (action === "insert" ? exists : !exists) {
      throw new MetadataRuleError("entryChanged");
    }
    const next: FieldEntry[] = field.entries.filter(
      (entry) => entry.key !== key,
    );
    if (action !== "delete") {
      next.push({
        key,
        value: field.shape === "byteMap" ? scalarText(content, "byteMap") : "",
      });
    }
    checkCollection(next, field.shape);
    const sdkKey = fieldKey(key, field.shape);
    let operation: ComponentMutation;
    if (field.shape === "byteMap") {
      operation = {
        kind: "mapDelta",
        value: [
          action === "delete"
            ? { kind: "delete", value: sdkKey }
            : {
                kind: action,
                value0: sdkKey,
                value1: fieldValue(content, "byteMap"),
              },
        ],
      };
    } else {
      if (action === "update") throw new MetadataRuleError("unsupported");
      operation = {
        kind: "setDelta",
        value: [{ kind: action, value: sdkKey }],
      };
    }
    this.#check();
    await conversation.updateMetadataField(ref, operation);
  }

  async #editOwn(
    changes: ReadonlyMap<number, string | undefined>,
    field: (id: number) => [MetadataField, MetadataFieldRef],
    conversation: Conversation,
  ): Promise<void> {
    const own = this.#session.inboxId;
    const updates: UserFieldUpdate[] = [];
    for (const [id, text] of changes) {
      const [current, ref] = field(id);
      if (!current.userField) throw new MetadataRuleError("unsupported");
      const value =
        text === undefined ? undefined : fieldValue(text, current.shape);
      const valueText = value === undefined ? "" : fieldValueText(value);
      if (
        current.present === (value !== undefined) &&
        current.scalar === valueText
      ) {
        continue;
      }
      if (value === undefined ? !allows(current, "delete") : !canSet(current)) {
        throw new MetadataRuleError("immutable");
      }
      const stored = await conversation.metadataValue(ref);
      if (
        isImmutable(current) &&
        (stored !== undefined || value === undefined)
      ) {
        throw new MetadataRuleError("immutable");
      }
      const entries: FieldEntry[] =
        stored?.kind === "map"
          ? stored.value
              .filter((entry) => keyText(entry.key) !== own)
              .map((entry) => ({
                key: keyText(entry.key),
                value: fieldValueText(entry.value),
              }))
          : [];
      if (value !== undefined) entries.push({ key: own, value: valueText });
      checkCollection(entries, current.shape);
      updates.push(
        value === undefined ? { field: ref } : { field: ref, value },
      );
    }
    if (updates.length > 0) {
      this.#check();
      await conversation.updateUserData(updates);
    }
  }
}
