import {
  Client,
  type ClientOptions,
  type PublicIdentity,
  type Signer,
} from "@xmtp/react-native-sdk";
import { NativeModules } from "react-native";

import { assert, equal } from "./assertions";
import { within } from "./async";
import type { FoundationResult, JourneyControl } from "./journey";

interface RuntimeFixture {
  readonly launchURL: string;
  readonly generation: number;
  reload(url: string, generation: number): Promise<void>;
}
interface RuntimeState {
  identity: PublicIdentity;
  backendUrl: string;
  path: string;
  latePath: string;
  installationId: string;
  conversationId: string;
  messageId: string;
  result: FoundationResult;
}

function fixture(): RuntimeFixture {
  const native = NativeModules.XmtpRuntimeFixture as RuntimeFixture | undefined;
  assert(
    native,
    "release consumer must install the private runtime fixture module",
  );
  return native;
}

export function runtimeLaunchURL(): string | undefined {
  return fixture().launchURL || undefined;
}

function options(backendUrl: string, dbPath?: string): ClientOptions {
  return {
    backend: { url: backendUrl },
    deviceSync: false,
    registration: { nonce: 9_007_199_254_740_993n },
    storage: {
      location: dbPath
        ? { dbPath, attachmentsDir: `${dbPath}.attachments` }
        : "default",
      label: "runtime-reload",
      encryptionKey: new Uint8Array(32).fill(71),
    },
  };
}
function signer(control: JourneyControl, identity: PublicIdentity): Signer {
  return {
    identity: async () => identity,
    kind: async () => ({ kind: "eoa" }),
    sign: async ({ text }) => ({
      kind: "ecdsa",
      value: new Uint8Array(
        (await control.call<{ bytes: number[] }>("sign", { text })).bytes,
      ),
    }),
  };
}
// Retain wrappers until the real native reload. No JS cleanup may close these
// handles on the successful path: that is the native teardown under assertion.
const retained: unknown[] = [];

export async function prepareRuntimeReload(
  control: JourneyControl,
  url: string,
  result: FoundationResult,
): Promise<never> {
  const native = fixture();
  equal(
    native.generation,
    0,
    "first journey must run before any fixture reload",
  );
  const setup = await control.call<{
    identity: PublicIdentity;
    backendUrl: string;
  }>("runtime/setup");
  const owner = await Client.create(
    signer(control, setup.identity),
    options(setup.backendUrl),
  );
  const group = await owner.conversations.createGroup([]);
  const messageId = await group.sendText(
    "native reload unacknowledged sentinel",
  );
  const stream = group.streamMessages();
  retained.push(owner, stream);
  for (;;) {
    const item = await within(stream.next(), "pre-reload default reader");
    assert(!item.done, "default reader must deliver reload sentinel");
    if (item.value.id === messageId) break;
  }
  const path = owner.storagePath;
  assert(path, "runtime owner must use native persistent storage");
  const latePath = `${path}.late-create`;
  const held = Client.create(
    {
      ...signer(control, setup.identity),
      async sign({ text }) {
        const response = await control.call<{ bytes: number[] }>(
          "runtime/hold-sign",
          { text },
        );
        return { kind: "ecdsa", value: new Uint8Array(response.bytes) };
      },
    },
    options(setup.backendUrl, latePath),
  ).then(
    () => control.call("runtime/stale", { kind: "late-create-resolved" }),
    () => control.call("runtime/stale", { kind: "late-create-rejected" }),
  );
  retained.push(held);
  await control.call("runtime/prepare", {
    ...setup,
    path,
    latePath,
    installationId: owner.installationId,
    conversationId: group.id,
    messageId,
    result,
  });
  try {
    await native.reload(`${url}&phase=reloaded`, native.generation);
  } catch (error) {
    await owner.end();
    throw error;
  }
  // This runtime is about to be invalidated. Its caller must never report success.
  return new Promise<never>(() => {});
}

export async function resumeAfterRuntimeReload(
  control: JourneyControl,
): Promise<FoundationResult> {
  equal(
    fixture().generation,
    1,
    "replacement must come from one native ReactHost reload in the same process",
  );
  const state = await control.call<RuntimeState>("runtime/resume");
  const wallet = signer(control, state.identity);
  const owner = await Client.create(
    wallet,
    options(state.backendUrl, state.path),
  );
  let late: Client | undefined;
  try {
    equal(
      owner.installationId,
      state.installationId,
      "replacement runtime must reopen the original native database",
    );
    const conversation = await owner.conversations.getById(
      state.conversationId,
    );
    assert(conversation, "native reload must retain the conversation");
    const reader = conversation.streamMessages();
    try {
      const item = await within(
        reader.next(),
        "replacement acquires old default reader ownership",
      );
      assert(
        !item.done,
        "replacement reader must recover unacknowledged message",
      );
      equal(
        item.value.id,
        state.messageId,
        "native teardown must release reader without acknowledging it",
      );
    } finally {
      await reader.end();
    }
    late = await within(
      Client.create(wallet, options(state.backendUrl, state.latePath)),
      "replacement opens late-constructor storage",
    );
    assert(
      await late.isRegistered(),
      "replacement remains usable while old signer response settles",
    );
    const status = await within(
      control.call<{
        staleCalls: number;
        heldSigns: number;
        signOutcome: string;
        responseOutcome: string;
        observationMs: number;
      }>("runtime/observe"),
      "released signer response settlement and bounded stale callback observation",
    );
    equal(
      status.signOutcome,
      "fulfilled",
      "released old signer must actually finish before reload success",
    );
    assert(
      status.responseOutcome === "finished" ||
        status.responseOutcome === "closed",
      "old signer HTTP response must finish or close before observation",
    );
    equal(
      status.observationMs,
      2000,
      "reload must observe late callbacks after signer transport settlement",
    );
    equal(
      status.heldSigns,
      1,
      "old native constructor must reach an unresolved wallet callback before reload",
    );
    equal(
      status.staleCalls,
      0,
      "old runtime callbacks must remain absent during the post-settlement observation window",
    );
    return {
      ...state.result,
      phases: [...state.result.phases, "runtime-reload"],
      assertions: [
        ...state.result.assertions,
        "release ReactHost invalidation/reload releases native default reader and preserves unacknowledged delivery; released signer and HTTP response settle; no old callback arrives during the following two-second observation",
      ],
    };
  } finally {
    await late?.end();
    await owner.end();
  }
}
