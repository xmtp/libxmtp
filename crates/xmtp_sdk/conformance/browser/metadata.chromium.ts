import { expect } from "vitest";

// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { generatePrivateKey } from "../../../../sdks/browser/node_modules/viem/_esm/accounts/generatePrivateKey.js";
// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { privateKeyToAccount } from "../../../../sdks/browser/node_modules/viem/_esm/accounts/privateKeyToAccount.js";
// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { toBytes } from "../../../../sdks/browser/node_modules/viem/_esm/utils/encoding/toBytes.js";
import * as Pure from "../../../../target/sdk-bridge-panic-fixture/typescript-pure/index";
import {
  CONTRACT_HASH,
  PROTOCOL_VERSION,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/contract.gen";
import * as sdk from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/index";
// The catalogue hook changes the worker's state, so the clients and the hook
// share one session to that worker. The package worker ends when idle.
import {
  Client as ProxyClient,
  sdkConformanceUseApplicationComponents,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/proxy.gen";
import { wrapClient } from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/public-client.gen";
import {
  currentProjection,
  lowerApplicationComponentDefinition,
  lowerSigner,
  publicError,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/public-values.gen";
import { MainSession } from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/runtime/bridge/main/session";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/runtime/bridge/wire";
import {
  hostOptions,
  publicClient,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/runtime/public/client";

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

function signer(): sdk.Signer {
  const account = privateKeyToAccount(generatePrivateKey());
  return {
    async identity() {
      return { identifier: account.address.toLowerCase(), kind: "ethereum" };
    },
    async kind() {
      return { kind: "eoa" };
    },
    async sign(request) {
      const signed = await account.signMessage({ message: request.text });
      return { kind: "ecdsa", value: Uint8Array.from(toBytes(signed)) };
    },
  };
}

async function useCatalogue(
  session: MainSession,
  catalogue: sdk.ApplicationComponentDefinition[] | undefined,
): Promise<void> {
  const projection = currentProjection();
  await sdkConformanceUseApplicationComponents(
    session,
    catalogue?.map((item) =>
      lowerApplicationComponentDefinition(item, projection),
    ),
  );
}

async function clientWith(
  session: MainSession,
  catalogue: sdk.ApplicationComponentDefinition[],
  backendURL: string,
): Promise<sdk.Client> {
  await useCatalogue(session, catalogue);
  try {
    const projection = currentProjection();
    const options: sdk.ClientOptions = {
      backend: { url: backendURL },
      storage: { location: "inMemory" },
      deviceSync: false,
    };
    const proxy = await ProxyClient.create(
      session,
      lowerSigner(signer(), projection),
      hostOptions(options, projection),
    ).catch((error: unknown) => {
      throw publicError(error);
    });
    return publicClient(wrapClient(proxy));
  } finally {
    await useCatalogue(session, undefined);
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

async function rejects(
  action: Promise<unknown>,
  variant: (error: unknown) => boolean,
  code: string,
  category: sdk.ErrorCategory,
): Promise<void> {
  const error = await action.then(
    () => undefined,
    (error: unknown) => error,
  );
  expect(variant(error)).toBe(true);
  const { details } = error as sdk.XmtpError;
  expect([details.code, details.category, details.retryable]).toStrictEqual([
    code,
    category,
    false,
  ]);
}

async function epoch(group: sdk.Group | sdk.Dm): Promise<bigint> {
  return (await group.debugInfo()).epoch;
}

// `toStrictEqual` compares byte contents and record types; `toEqual` would
// accept any two byte values.
// verifies: META-069, META-070, META-071, META-072, META-073
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
  let alix: sdk.Client | undefined;
  let bo: sdk.Client | undefined;
  try {
    await session.ready();
    alix = await clientWith(session, alixCatalogue, backendURL);
    bo = await clientWith(session, boCatalogue, backendURL);
    expect(alix.serverConfiguration.applicationComponents).toStrictEqual(
      alixCatalogue,
    );
    const group = await alix.conversations.createGroup([bo.inboxId]);
    await bo.conversations.sync();
    const boGroup = await bo.conversations.getById(group.id);
    if (!(boGroup instanceof sdk.Group))
      throw new Error("Bo did not join the group");

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
    const displayName = Pure.metadataFieldRef("userDisplayName");
    const groupName = Pure.metadataFieldRef("groupName");
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
    await boGroup.updateMetadataField(field(STATUS), replace(string("hello")));
    await group.sync();
    await group.updateMetadataField(
      field(AVATAR, "avatar"),
      replace(bytes(1, 2, 3)),
    );
    await group.updateMetadataField(groupName, replace(string("Team")));
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
        [alix.inboxId, []],
        [bo.inboxId, boProfile],
      ]),
    );
    expect(await group.userData([], undefined)).toStrictEqual(
      new Map([
        [alix.inboxId, []],
        [bo.inboxId, []],
      ]),
    );
    expect(await group.userData(undefined, [])).toStrictEqual(new Map());
    expect(await group.userData([field(NICKNAME)], [bo.inboxId])).toStrictEqual(
      new Map([[bo.inboxId, boProfile.slice(1)]]),
    );

    // Denials are typed and commit nothing.
    await rejects(
      boGroup.updateMetadataField(field(TOPIC), replace(string("x"))),
      (error) => error instanceof sdk.XmtpError.PermissionDenied,
      "PermissionDenied",
      "conversation",
    );
    await rejects(
      boGroup.updateUserData([
        { field: displayName, value: string("Bobby") },
        { field: displayName, value: undefined },
      ]),
      (error) => error instanceof sdk.XmtpError.DuplicateField,
      "DuplicateField",
      "input",
    );
    expect(await epoch(boGroup)).toBe(before + 1n);
    expect(await boGroup.metadataValue(field(TOPIC))).toBeUndefined();

    // A DM holds the pair's profiles and its DM fields, never group-only ones.
    const dm = await alix.conversations.createDm(bo.inboxId);
    const dmIds = (await dm.metadataFields()).map((d) => d.field.componentId);
    expect(dmIds).toContain(DISPLAY_NAME);
    expect(dmIds).toContain(NICKNAME);
    expect(dmIds).not.toContain(AVATAR);
    await dm.updateUserData([{ field: displayName, value: string("Alix") }]);
    expect(await dm.userData(undefined, undefined)).toStrictEqual(
      new Map([
        [alix.inboxId, [{ field: displayName, value: string("Alix") }]],
        [bo.inboxId, []],
      ]),
    );
    await rejects(
      dm.updateMetadataField(field(AVATAR), replace(bytes(1))),
      (error) => error instanceof sdk.XmtpError.UnknownField,
      "UnknownField",
      "input",
    );
  } finally {
    await alix?.end();
    await bo?.end();
    worker.terminate();
  }
}
