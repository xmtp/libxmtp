import { expect } from "vitest";

// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { generatePrivateKey } from "../../../../sdks/browser/node_modules/viem/_esm/accounts/generatePrivateKey.js";
// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { privateKeyToAccount } from "../../../../sdks/browser/node_modules/viem/_esm/accounts/privateKeyToAccount.js";
// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { toBytes } from "../../../../sdks/browser/node_modules/viem/_esm/utils/encoding/toBytes.js";
import * as Pure from "../../../../target/sdk-bridge-panic-fixture/typescript-pure/index";
import { mainEncoder } from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/codec.main.gen";
import {
  CONTRACT_HASH,
  PROTOCOL_VERSION,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/contract.gen";
import { Client } from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/index";
import { MainSession } from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/runtime/bridge/main/session";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/runtime/bridge/wire";
import * as B from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/xmtp_sdk";

// Alix and Bo read one group through different backend catalogues, so a
// name labels a field for one reader only and the component ID identifies
// it.
const STATUS = 0xc001;
const NICKNAME = 0xc002;
const TOPIC = 0xc003;
const AVATAR = 0xc004;
const LATER = 0xc005;
const DISPLAY_NAME = 0x800c;

const Base = B.MetadataBasePolicy;
const Type = B.MetadataComponentType;
const nicknameType = Type.Map.new({
  keyType: B.MetadataKeyType.InboxId,
  valueType: B.MetadataScalarType.String,
});

function field(componentId: number, name?: string): B.MetadataFieldRef {
  return { componentId, name };
}

function permissions(base: B.MetadataBasePolicy): B.ComponentPermissions {
  const policy = B.MetadataPolicy.Base.new(base);
  return { insert: policy, update: policy, delete: policy };
}

function definition(
  componentId: number,
  name: string,
  componentType: B.MetadataComponentType,
  base: B.MetadataBasePolicy,
  inDms: boolean,
): B.ApplicationComponentDefinition {
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
  definition(STATUS, "status", Type.String.new(), Base.Allow.new(), true),
  definition(
    NICKNAME,
    "nickname",
    nicknameType,
    Base.AllowIfSelfOrNonMember.new(),
    true,
  ),
  definition(TOPIC, "topic", Type.String.new(), Base.AllowIfAdmin.new(), true),
  definition(AVATAR, "avatar", Type.Bytes.new(), Base.Allow.new(), false),
  definition(
    LATER,
    "later",
    Type.Unknown.new({ tag: 99 }),
    Base.Allow.new(),
    true,
  ),
];
// Bo's catalogue gives `status` to another field and names STATUS after a
// well-known field, with a type and policy the group never committed.
const boCatalogue = [
  definition(STATUS, "GROUP_NAME", Type.Bytes.new(), Base.Deny.new(), true),
  definition(AVATAR, "status", Type.Bytes.new(), Base.Allow.new(), false),
];

function signer(): B.Signer {
  const account = privateKeyToAccount(generatePrivateKey());
  return {
    async identity() {
      return {
        identifier: account.address.toLowerCase(),
        kind: B.PublicIdentityKind.Ethereum,
      };
    },
    async kind() {
      return B.SignerKind.Eoa.new();
    },
    async sign(request: { text: string }) {
      const signed = await account.signMessage({ message: request.text });
      return B.Signature.Ecdsa.new(Uint8Array.from(toBytes(signed)).buffer);
    },
  };
}

async function useCatalogue(
  session: MainSession,
  catalogue: B.ApplicationComponentDefinition[] | undefined,
): Promise<void> {
  await session.call("sdkConformanceUseApplicationComponents", () => [
    mainEncoder(session).convert(
      {
        kind: "optional",
        inner: {
          kind: "sequence",
          inner: { kind: "record", name: "ApplicationComponentDefinition" },
        },
      },
      catalogue,
    ),
  ]);
}

async function clientWith(
  session: MainSession,
  catalogue: B.ApplicationComponentDefinition[],
  backendURL: string,
): Promise<Client> {
  await useCatalogue(session, catalogue);
  try {
    return await Client.create(session, signer(), {
      backend: B.BackendSource.Options.new({
        options: {
          url: backendURL,
          appVersion: undefined,
          credential: undefined,
          credentials: undefined,
        },
      }),
      storage: {
        location: B.StorageLocation.InMemory.new(),
        label: undefined,
        pool: undefined,
        singleConnection: false,
      },
      deviceSync: false,
      allowOffline: false,
      registration: { auto: true, nonce: undefined },
      forkRecovery: undefined,
      workers: undefined,
    });
  } finally {
    await useCatalogue(session, undefined);
  }
}

function string(value: string): B.FieldValue {
  return B.FieldValue.String.new(value);
}

function bytes(...values: number[]): B.FieldValue {
  return B.FieldValue.Bytes.new(Uint8Array.from(values).buffer);
}

function scalar(value: B.FieldValue): B.MetadataValue {
  return B.MetadataValue.Scalar.new(value);
}

async function rejects(
  action: Promise<unknown>,
  variant: (error: unknown) => boolean,
  code: string,
  category: B.ErrorCategory,
): Promise<void> {
  const error = await action.then(
    () => undefined,
    (error: unknown) => error,
  );
  expect(variant(error)).toBe(true);
  const details = (error as { inner: [B.ErrorDetails] }).inner[0];
  expect([details.code, details.category, details.retryable]).toStrictEqual([
    code,
    category,
    false,
  ]);
}

async function epoch(group: B.GroupLike): Promise<bigint> {
  return (await group.debugInfo()).epoch;
}

// `toStrictEqual` compares ArrayBuffer contents and record types; `toEqual`
// would accept any two byte values.
export async function checkMetadataFields(backendURL: string): Promise<void> {
  await Pure.initPureWasm();
  const worker = new Worker(new URL("./metadata.worker.ts", import.meta.url), {
    type: "module",
  });
  const endpoint: WireEndpoint = {
    postMessage(message, transfer) {
      worker.postMessage(message, { transfer });
    },
    onMessage(handler) {
      worker.addEventListener("message", (event: MessageEvent<WireMessage>) =>
        handler(event.data),
      );
    },
    onExit(handler) {
      worker.addEventListener("error", handler);
    },
    terminate() {
      worker.terminate();
    },
  };
  const session = new MainSession(endpoint, PROTOCOL_VERSION, CONTRACT_HASH);
  let alix: Client | undefined;
  let bo: Client | undefined;
  try {
    await session.ready();
    alix = await clientWith(session, alixCatalogue, backendURL);
    bo = await clientWith(session, boCatalogue, backendURL);
    expect(alix.serverConfiguration().applicationComponents).toStrictEqual(
      alixCatalogue,
    );
    const group = await alix
      .conversations()
      .createGroup([bo.inboxId()], undefined);
    await bo.conversations().sync();
    const joined = await bo.conversations().getById(group.id());
    if (joined?.tag !== B.Conversation_Tags.Group)
      throw new Error("Bo did not join the group");
    const boGroup = joined.inner.group;

    // Descriptors: the committed type and policies with each reader's labels.
    const rows: [
      number,
      B.MetadataComponentType,
      B.MetadataBasePolicy,
      boolean,
    ][] = [
      [STATUS, Type.String.new(), Base.Allow.new(), false],
      [NICKNAME, nicknameType, Base.AllowIfSelfOrNonMember.new(), true],
      [TOPIC, Type.String.new(), Base.AllowIfAdmin.new(), false],
      [AVATAR, Type.Bytes.new(), Base.Allow.new(), false],
    ];
    const application = (labels: (string | undefined)[]) =>
      rows.map(([id, componentType, base, isUserField], index) => ({
        field: field(id, labels[index]),
        componentType,
        permissions: permissions(base),
        isUserField,
      }));
    const displayName = Pure.metadataFieldRef(
      Pure.WellKnownMetadataField.UserDisplayName,
    );
    const groupName = Pure.metadataFieldRef(
      Pure.WellKnownMetadataField.GroupName,
    );
    expect(displayName).toStrictEqual(field(DISPLAY_NAME, "USER_DISPLAY_NAME"));
    const alixFields = await group.metadataFields();
    expect(
      alixFields
        .slice(0, 8)
        .map((descriptor) => [descriptor.field, descriptor.isUserField]),
    ).toStrictEqual([
      [groupName, false],
      [field(0x8005, "GROUP_DESCRIPTION"), false],
      [field(0x8006, "GROUP_IMAGE_URL"), false],
      [field(0x8007, "MESSAGE_DISAPPEAR_FROM_NS"), false],
      [field(0x8008, "MESSAGE_DISAPPEAR_IN_NS"), false],
      [field(0x8009, "APP_DATA"), false],
      [displayName, true],
      [field(0x800d, "GROUP_IMAGE"), false],
    ]);
    expect(alixFields.slice(8)).toStrictEqual(
      application(["status", "nickname", "topic", "avatar"]),
    );
    expect((await boGroup.metadataFields()).slice(8)).toStrictEqual(
      application(["GROUP_NAME", undefined, undefined, "status"]),
    );
    expect((await group.metadataField("status"))?.field).toStrictEqual(
      field(STATUS, "status"),
    );
    expect((await boGroup.metadataField("status"))?.field).toStrictEqual(
      field(AVATAR, "status"),
    );
    expect((await boGroup.metadataField("GROUP_NAME"))?.field).toStrictEqual(
      groupName,
    );
    expect(await group.metadataField("later")).toBeUndefined();

    // Values: request order from one snapshot, each with the reader's label.
    await boGroup.updateMetadataField(
      field(STATUS),
      B.ComponentMutation.Replace.new(string("hello")),
    );
    await group.sync();
    await group.updateMetadataField(
      field(AVATAR, "avatar"),
      B.ComponentMutation.Replace.new(bytes(1, 2, 3)),
    );
    await group.updateMetadataField(
      groupName,
      B.ComponentMutation.Replace.new(string("Team")),
    );
    await boGroup.sync();
    expect(
      await boGroup.metadataValues([
        field(AVATAR, "status"),
        field(STATUS, "status"),
        groupName,
        field(TOPIC),
      ]),
    ).toStrictEqual([
      { field: field(AVATAR, "status"), value: scalar(bytes(1, 2, 3)) },
      { field: field(STATUS, "GROUP_NAME"), value: scalar(string("hello")) },
      { field: groupName, value: scalar(string("Team")) },
      { field: field(TOPIC), value: undefined },
    ]);
    expect(await boGroup.metadataValues([])).toStrictEqual([]);

    // Profiles: one commit for two fields; a repeat commits nothing.
    const before = await epoch(boGroup);
    await boGroup.updateUserData([
      { field: displayName, value: string("Bo") },
      { field: field(NICKNAME), value: string("B") },
    ]);
    expect(await epoch(boGroup)).toBe(before + 1n);
    await boGroup.updateUserData([{ field: displayName, value: string("Bo") }]);
    expect(await epoch(boGroup)).toBe(before + 1n);
    await group.sync();
    const boProfile = [
      { field: displayName, value: string("Bo") },
      { field: field(NICKNAME, "nickname"), value: string("B") },
    ];
    expect(await group.userData(undefined, undefined)).toStrictEqual(
      new Map([
        [alix.inboxId(), []],
        [bo.inboxId(), boProfile],
      ]),
    );
    expect(await group.userData([], undefined)).toStrictEqual(
      new Map([
        [alix.inboxId(), []],
        [bo.inboxId(), []],
      ]),
    );
    expect(await group.userData(undefined, [])).toStrictEqual(new Map());
    expect(
      await group.userData([field(NICKNAME)], [bo.inboxId()]),
    ).toStrictEqual(new Map([[bo.inboxId(), boProfile.slice(1)]]));

    // Denials are typed and commit nothing.
    await rejects(
      boGroup.updateMetadataField(
        field(TOPIC),
        B.ComponentMutation.Replace.new(string("x")),
      ),
      (error) => B.XmtpError.PermissionDenied.instanceOf(error),
      "PermissionDenied",
      B.ErrorCategory.Conversation,
    );
    await rejects(
      boGroup.updateUserData([
        { field: displayName, value: string("Bobby") },
        { field: displayName, value: undefined },
      ]),
      (error) => B.XmtpError.DuplicateField.instanceOf(error),
      "DuplicateField",
      B.ErrorCategory.Input,
    );
    expect(await epoch(boGroup)).toBe(before + 1n);
    expect(await boGroup.metadataValue(field(TOPIC))).toBeUndefined();

    // A DM holds the pair's profiles and its DM fields, never group-only ones.
    const dm = await alix.conversations().createDm(bo.inboxId(), undefined);
    const dmIds = (await dm.metadataFields()).map((d) => d.field.componentId);
    expect(dmIds).toContain(DISPLAY_NAME);
    expect(dmIds).toContain(NICKNAME);
    expect(dmIds).not.toContain(AVATAR);
    await dm.updateUserData([{ field: displayName, value: string("Alix") }]);
    expect(await dm.userData(undefined, undefined)).toStrictEqual(
      new Map([
        [alix.inboxId(), [{ field: displayName, value: string("Alix") }]],
        [bo.inboxId(), []],
      ]),
    );
    await rejects(
      dm.updateMetadataField(
        field(AVATAR),
        B.ComponentMutation.Replace.new(bytes(1)),
      ),
      (error) => B.XmtpError.UnknownField.instanceOf(error),
      "UnknownField",
      B.ErrorCategory.Input,
    );
  } finally {
    await alix?.end();
    await bo?.end();
    worker.terminate();
  }
}
