import type * as sdk from "../../../../target/sdk-conformance/typescript-napi/index.ts";

// Alix and Bo read one group through different backend catalogues, so a
// name labels a field for one reader only and the component ID identifies
// it.
export const STATUS = 0xc001;
export const NICKNAME = 0xc002;
export const TOPIC = 0xc003;
export const AVATAR = 0xc004;
const LATER = 0xc005;
export const BYTE_MAP = 0xc006;
export const BYTE_SET = 0xc007;
export const DISPLAY_NAME = 0x800c;

type Policy = sdk.MetadataBasePolicy;

export const allow: Policy = { kind: "allow" };
const deny: Policy = { kind: "deny" };
export const allowIfAdmin: Policy = { kind: "allowIfAdmin" };
export const allowIfSelfOrNonMember: Policy = {
  kind: "allowIfSelfOrNonMember",
};
export const stringType: sdk.MetadataComponentType = { kind: "string" };
export const bytesType: sdk.MetadataComponentType = { kind: "bytes" };
export const mapType: sdk.MetadataComponentType = {
  kind: "map",
  keyType: "bytes",
  valueType: "bytes",
};
export const setType: sdk.MetadataComponentType = {
  kind: "set",
  keyType: "bytes",
};
export const nicknameType: sdk.MetadataComponentType = {
  kind: "map",
  keyType: "inboxId",
  valueType: "string",
};

export function field(
  componentId: number,
  name?: string,
): sdk.MetadataFieldRef {
  return { componentId, name };
}

export function permissions(base: Policy): sdk.ComponentPermissions {
  const policy: sdk.MetadataPolicy = { kind: "base", value: base };
  return { insert: policy, update: policy, delete: policy };
}

function definition(
  componentId: number,
  name: string,
  componentType: sdk.MetadataComponentType,
  base: Policy,
  inDms: boolean,
): sdk.ApplicationComponentDefinition {
  return {
    componentId,
    name,
    componentType,
    permissions: permissions(base),
    inGroups: true,
    inDms,
  };
}

// `later` has a type tag no SDK knows, so no conversation registers it.
export const alixCatalogue = [
  definition(STATUS, "status", stringType, allow, true),
  definition(NICKNAME, "nickname", nicknameType, allowIfSelfOrNonMember, true),
  definition(TOPIC, "topic", stringType, allowIfAdmin, true),
  definition(AVATAR, "avatar", bytesType, allow, false),
  definition(BYTE_MAP, "byte_map", mapType, allow, false),
  definition(BYTE_SET, "byte_set", setType, allow, false),
  definition(LATER, "later", { kind: "unknown", tag: 99 }, allow, true),
];
// Bo's catalogue gives `status` to another field and names STATUS after a
// well-known field, with a type and policy the group never committed.
export const boCatalogue = [
  definition(STATUS, "GROUP_NAME", bytesType, deny, true),
  definition(AVATAR, "status", bytesType, allow, false),
];

export function string(value: string): sdk.FieldValue {
  return { kind: "string", value };
}

export function bytes(...values: number[]): sdk.FieldValue {
  return { kind: "bytes", value: Uint8Array.from(values) };
}

export function scalar(value: sdk.FieldValue): sdk.MetadataValue {
  return { kind: "scalar", value };
}

export function replace(value: sdk.FieldValue): sdk.ComponentMutation {
  return { kind: "replace", value };
}

// The host supplies its assertion function. This module imports no SDK at runtime.
export async function collectionScenario(
  group: Pick<sdk.Group, "updateMetadataField" | "metadataValue" | "mapValue">,
  equal: (actual: unknown, expected: unknown) => void,
): Promise<void> {
  const key: sdk.FieldKey = { kind: "bytes", value: Uint8Array.of(0, 255) };
  await group.updateMetadataField(field(BYTE_MAP), {
    kind: "mapDelta",
    value: [{ kind: "insert", value0: key, value1: bytes(0, 128, 255) }],
  });
  equal(await group.metadataValue(field(BYTE_MAP)), {
    kind: "map",
    value: [{ key, value: bytes(0, 128, 255) }],
  });
  await group.updateMetadataField(field(BYTE_MAP), {
    kind: "mapDelta",
    value: [{ kind: "update", value0: key, value1: bytes(4, 0) }],
  });
  equal(await group.mapValue(field(BYTE_MAP), key), bytes(4, 0));
  await group.updateMetadataField(field(BYTE_MAP), {
    kind: "mapDelta",
    value: [{ kind: "delete", value: key }],
  });
  equal(await group.metadataValue(field(BYTE_MAP)), { kind: "map", value: [] });
  await group.updateMetadataField(field(BYTE_SET), {
    kind: "setDelta",
    value: [{ kind: "insert", value: key }],
  });
  equal(await group.metadataValue(field(BYTE_SET)), {
    kind: "set",
    value: [key],
  });
  await group.updateMetadataField(field(BYTE_SET), {
    kind: "setDelta",
    value: [{ kind: "delete", value: key }],
  });
  equal(await group.metadataValue(field(BYTE_SET)), { kind: "set", value: [] });
}
