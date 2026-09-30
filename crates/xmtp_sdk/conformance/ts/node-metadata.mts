import assert from "node:assert/strict";

import * as sdk from "../../../../target/sdk-conformance/typescript-napi/public-api.gen.ts";

// Alix and Bo read one group through different backend catalogues, so a
// name labels a field for one reader only and the component ID identifies
// it.
const STATUS = 0xc001;
const NICKNAME = 0xc002;
const TOPIC = 0xc003;
const AVATAR = 0xc004;
const LATER = 0xc005;
const DISPLAY_NAME = 0x800c;

type Policy = sdk.MetadataBasePolicy;

const allow: Policy = { kind: "allow" };
const deny: Policy = { kind: "deny" };
const allowIfAdmin: Policy = { kind: "allowIfAdmin" };
const allowIfSelfOrNonMember: Policy = { kind: "allowIfSelfOrNonMember" };
const stringType: sdk.MetadataComponentType = { kind: "string" };
const bytesType: sdk.MetadataComponentType = { kind: "bytes" };
const nicknameType: sdk.MetadataComponentType = {
  kind: "map",
  keyType: "inboxId",
  valueType: "string",
};

function field(componentId: number, name?: string): sdk.MetadataFieldRef {
  return { componentId, name };
}

function permissions(base: Policy): sdk.ComponentPermissions {
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
const alixCatalogue = [
  definition(STATUS, "status", stringType, allow, true),
  definition(NICKNAME, "nickname", nicknameType, allowIfSelfOrNonMember, true),
  definition(TOPIC, "topic", stringType, allowIfAdmin, true),
  definition(AVATAR, "avatar", bytesType, allow, false),
  definition(LATER, "later", { kind: "unknown", tag: 99 }, allow, true),
];
// Bo's catalogue gives `status` to another field and names STATUS after a
// well-known field, with a type and policy the group never committed.
const boCatalogue = [
  definition(STATUS, "GROUP_NAME", bytesType, deny, true),
  definition(AVATAR, "status", bytesType, allow, false),
];

async function clientWith(
  catalogue: sdk.ApplicationComponentDefinition[],
  options: sdk.ClientOptions,
): Promise<sdk.Client> {
  await sdk.sdkConformanceUseApplicationComponents(catalogue);
  try {
    return await sdk.Client.create(await sdk.generateLocalSigner(), {
      ...options,
      storage: { ...options.storage, location: "inMemory" },
    });
  } finally {
    await sdk.sdkConformanceUseApplicationComponents(undefined);
  }
}

function string(value: string): sdk.FieldValue {
  return { kind: "string", value };
}

function bytes(...values: number[]): sdk.FieldValue {
  return { kind: "bytes", value: Uint8Array.from(values) };
}

function scalar(value: sdk.FieldValue): sdk.MetadataValue {
  return { kind: "scalar", value };
}

function replace(value: sdk.FieldValue): sdk.ComponentMutation {
  return { kind: "replace", value };
}

function assertKind(error: unknown, code: string, category: sdk.ErrorCategory) {
  const { details } = error as sdk.XmtpError;
  assert.deepEqual(
    [details.code, details.category, details.retryable],
    [code, category, false],
  );
  return true;
}

async function epoch(group: sdk.Group | sdk.Dm): Promise<bigint> {
  return (await group.debugInfo()).epoch;
}

export async function metadataFields(
  options: sdk.ClientOptions,
): Promise<void> {
  const alix = await clientWith(alixCatalogue, options);
  const bo = await clientWith(boCatalogue, options);
  assert.deepEqual(
    alix.serverConfiguration.applicationComponents,
    alixCatalogue,
  );
  const group = await alix.conversations.createGroup([bo.inboxId]);
  await bo.conversations.sync();
  const boGroup = await bo.conversations.getById(group.id);
  assert.ok(boGroup instanceof sdk.Group);

  // Descriptors: the committed type and policies with each reader's labels.
  const rows: [number, sdk.MetadataComponentType, Policy, boolean][] = [
    [STATUS, stringType, allow, false],
    [NICKNAME, nicknameType, allowIfSelfOrNonMember, true],
    [TOPIC, stringType, allowIfAdmin, false],
    [AVATAR, bytesType, allow, false],
  ];
  const application = (labels: (string | undefined)[]) =>
    rows.map(([id, componentType, base, isUserField], index) => ({
      field: field(id, labels[index]),
      componentType,
      permissions: permissions(base),
      isUserField,
    }));
  const displayName = sdk.metadataFieldRef("userDisplayName");
  const groupName = sdk.metadataFieldRef("groupName");
  assert.deepEqual(displayName, field(DISPLAY_NAME, "USER_DISPLAY_NAME"));
  const alixFields = await group.metadataFields();
  assert.deepEqual(
    alixFields
      .slice(0, 8)
      .map((descriptor) => [descriptor.field, descriptor.isUserField]),
    [
      [groupName, false],
      [field(0x8005, "GROUP_DESCRIPTION"), false],
      [field(0x8006, "GROUP_IMAGE_URL"), false],
      [field(0x8007, "MESSAGE_DISAPPEAR_FROM_NS"), false],
      [field(0x8008, "MESSAGE_DISAPPEAR_IN_NS"), false],
      [field(0x8009, "APP_DATA"), false],
      [displayName, true],
      [field(0x800d, "GROUP_IMAGE"), false],
    ],
  );
  assert.deepEqual(
    alixFields.slice(8),
    application(["status", "nickname", "topic", "avatar"]),
  );
  assert.deepEqual(
    (await boGroup.metadataFields()).slice(8),
    application(["GROUP_NAME", undefined, undefined, "status"]),
  );
  assert.deepEqual(
    (await group.metadataField("status"))?.field,
    field(STATUS, "status"),
  );
  assert.deepEqual(
    (await boGroup.metadataField("status"))?.field,
    field(AVATAR, "status"),
  );
  assert.deepEqual(
    (await boGroup.metadataField("GROUP_NAME"))?.field,
    groupName,
  );
  assert.equal(await group.metadataField("later"), undefined);

  // Values: request order from one snapshot, each with the reader's label.
  await boGroup.updateMetadataField(field(STATUS), replace(string("hello")));
  await group.sync();
  await group.updateMetadataField(
    field(AVATAR, "avatar"),
    replace(bytes(1, 2, 3)),
  );
  await group.updateMetadataField(groupName, replace(string("Team")));
  await boGroup.sync();
  assert.deepEqual(
    await boGroup.metadataValues([
      field(AVATAR, "status"),
      field(STATUS, "status"),
      groupName,
      field(TOPIC),
    ]),
    [
      { field: field(AVATAR, "status"), value: scalar(bytes(1, 2, 3)) },
      { field: field(STATUS, "GROUP_NAME"), value: scalar(string("hello")) },
      { field: groupName, value: scalar(string("Team")) },
      { field: field(TOPIC), value: undefined },
    ],
  );
  assert.deepEqual(await boGroup.metadataValues([]), []);

  // Profiles: one commit for two fields; a repeat commits nothing.
  const before = await epoch(boGroup);
  await boGroup.updateUserData([
    { field: displayName, value: string("Bo") },
    { field: field(NICKNAME), value: string("B") },
  ]);
  assert.equal(await epoch(boGroup), before + 1n);
  await boGroup.updateUserData([{ field: displayName, value: string("Bo") }]);
  assert.equal(await epoch(boGroup), before + 1n);
  await group.sync();
  const boProfile = [
    { field: displayName, value: string("Bo") },
    { field: field(NICKNAME, "nickname"), value: string("B") },
  ];
  assert.deepEqual(
    await group.userData(undefined, undefined),
    new Map([
      [alix.inboxId, []],
      [bo.inboxId, boProfile],
    ]),
  );
  assert.deepEqual(
    await group.userData([], undefined),
    new Map([
      [alix.inboxId, []],
      [bo.inboxId, []],
    ]),
  );
  assert.deepEqual(await group.userData(undefined, []), new Map());
  assert.deepEqual(
    await group.userData([field(NICKNAME)], [bo.inboxId]),
    new Map([[bo.inboxId, boProfile.slice(1)]]),
  );

  // Denials are typed and commit nothing.
  await assert.rejects(
    boGroup.updateMetadataField(field(TOPIC), replace(string("x"))),
    (error) =>
      error instanceof sdk.XmtpError.PermissionDenied &&
      assertKind(error, "PermissionDenied", "conversation"),
  );
  await assert.rejects(
    boGroup.updateUserData([
      { field: displayName, value: string("Bobby") },
      { field: displayName, value: undefined },
    ]),
    (error) =>
      error instanceof sdk.XmtpError.DuplicateField &&
      assertKind(error, "DuplicateField", "input"),
  );
  assert.equal(await epoch(boGroup), before + 1n);
  assert.equal(await boGroup.metadataValue(field(TOPIC)), undefined);

  // A DM holds the pair's profiles and its DM fields, never group-only ones.
  const dm = await alix.conversations.createDm(bo.inboxId);
  const dmIds = (await dm.metadataFields()).map((d) => d.field.componentId);
  assert.ok(dmIds.includes(DISPLAY_NAME) && dmIds.includes(NICKNAME));
  assert.ok(!dmIds.includes(AVATAR));
  await dm.updateUserData([{ field: displayName, value: string("Alix") }]);
  assert.deepEqual(
    await dm.userData(undefined, undefined),
    new Map([
      [alix.inboxId, [{ field: displayName, value: string("Alix") }]],
      [bo.inboxId, []],
    ]),
  );
  await assert.rejects(
    dm.updateMetadataField(field(AVATAR), replace(bytes(1))),
    (error) =>
      error instanceof sdk.XmtpError.UnknownField &&
      assertKind(error, "UnknownField", "input"),
  );

  await alix.end();
  await bo.end();
}
