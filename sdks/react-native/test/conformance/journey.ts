import {
  Client,
  XmtpError,
  decodeEncodedContent,
  encodeEncodedContent,
  generateInboxId,
  sdkVersion,
  type ClientOptions,
  type ContentCodec,
  type PublicIdentity,
  type Signer,
} from "@xmtp/react-native-sdk";
import { XmtpProvider } from "@xmtp/react-native-sdk/react";
import { NativeModules, Platform } from "react-native";

import { assert, bytesEqual, equal } from "./assertions";
import { within } from "./async";
import { boundaryJourney } from "./boundary";
import { contentVectorJourney } from "./content-vectors";
import { listenerJourney } from "./events";

export interface JourneyControl {
  call<Result>(operation: string, input?: unknown): Promise<Result>;
}

/**
 * The runner picks the set from `XMTP_RN_JOURNEYS`. `pr` (the default) runs
 * the RN-only checks. `full` adds the runtime reload and the cold background
 * launch, which post-merge and release runs select.
 */
export type JourneySet = "pr" | "full";

export const fixtureCodec: ContentCodec<Uint8Array> = {
  type: {
    authorityId: "xmtp.org",
    typeId: "rn-conformance",
    versionMajor: 1,
    versionMinor: 0,
  },
  encode: (content) => ({ type: fixtureCodec.type, content }),
  decode: (envelope) => envelope.content,
  shouldPush: () => false,
};

export interface FoundationResult {
  readonly journeys: JourneySet;
  readonly assertions: readonly string[];
  readonly phases: readonly string[];
  readonly inboxId: string;
  readonly conversationId: string;
  readonly outboundMessageId: string;
  readonly inboundMessageId: string;
}

/** The same installed-package journey runs in bare RN and Expo development builds. */
export async function foundationJourney(
  control: JourneyControl,
  conformance = false,
): Promise<FoundationResult> {
  const evidence: string[] = [];
  const phases: string[] = [];
  const setup = await control.call<{
    identity: PublicIdentity;
    backendUrl: string;
    peerInboxId: string;
    outboundText: string;
    inboundText: string;
    journeys: JourneySet;
    sdkVersion: string;
  }>("setup");
  assert(
    setup.journeys === "pr" || setup.journeys === "full",
    "runner must select the pr or full journey set",
  );
  equal(
    sdkVersion(),
    setup.sdkVersion,
    "RN and Node native SDK versions must match",
  );
  const signer: Signer = {
    identity: async () => setup.identity,
    kind: async () => ({ kind: "eoa" }),
    sign: async (request) => {
      const signed = await control.call<{ bytes: number[] }>("sign", {
        text: request.text,
      });
      return { kind: "ecdsa", value: new Uint8Array(signed.bytes) };
    },
  };

  // Fixed Rust identity vector, independent of the value returned by JSI.
  equal(
    generateInboxId(
      {
        kind: "ethereum",
        identifier: "0xabcdef0000000000000000000000000000000000",
      },
      9_007_199_254_740_993n,
    ),
    "7388e86684247cde39d20ef985b6999cb657325d1576c28b74670a5392228913",
    "JSI must preserve uint64 values above Number.MAX_SAFE_INTEGER",
  );
  evidence.push("native uint64 identity vector");

  for (const content of [
    new Uint8Array([91, 0, 128, 255, 92]).subarray(1, 4),
    new Uint8Array([91, 92]).subarray(1, 1),
  ]) {
    const envelope = {
      type: {
        authorityId: "xmtp.org",
        typeId: "rn-boundary",
        versionMajor: 1,
        versionMinor: 0,
      },
      content,
      parameters: new Map([["boundary", "sliced bytes"]]),
      fallback: undefined,
    };
    const restored = decodeEncodedContent(encodeEncodedContent(envelope));
    bytesEqual(
      restored.content,
      Array.from(content),
      "native envelope must retain byte view bounds",
    );
    equal(
      restored.parameters?.get("boundary"),
      "sliced bytes",
      "native envelope must retain map entries",
    );
  }
  evidence.push("native sliced and empty byte envelopes");

  const options: ClientOptions = {
    backend: { url: setup.backendUrl },
    deviceSync: false,
    codecs: [fixtureCodec],
    attachments: { allowPrivateNetwork: true },
    registration: { nonce: 9_007_199_254_740_993n },
    storage: {
      location: "default",
      encryptionKey: new Uint8Array(32).fill(67),
    },
  };
  let client: Client | undefined;
  let reopened: Client | undefined;
  try {
    client = await Client.create(signer, options);
    assert(
      await client.isRegistered(),
      "Client.create must finish registration",
    );
    const inboxId = client.inboxId;
    const installationId = client.installationId;
    const storagePath = client.storagePath;
    const raw = await (
      NativeModules.XmtpMigrationFixture as {
        osStorage(): Promise<{
          filesDirectory?: string;
          applicationSupportDirectory?: string;
          bundleId?: string;
        }>;
      }
    ).osStorage();
    const root =
      Platform.OS === "android"
        ? `${raw.filesDirectory}/xmtp_db`
        : `${raw.applicationSupportDirectory}/${raw.bundleId}/xmtp`;
    assert(
      root.startsWith("/") && !root.includes("undefined"),
      "private native fixture must report raw OS sandbox roots",
    );
    assert(
      storagePath && storagePath.startsWith(`${root}/`),
      "default native database must be inside the independently derived OS sandbox root",
    );
    assert(
      storagePath.endsWith(`/${inboxId.toLowerCase()}/xmtp.db3`),
      "default native database must retain the per-inbox layout",
    );
    const pending = await client.attachments.create({
      kind: "bytes",
      bytes: new Uint8Array([0, 128, 255]),
      filename: "sandbox.bin",
      mimeType: "application/octet-stream",
    });
    const localPath = await client.attachments.localPath(
      pending.remoteAttachment,
    );
    const attachmentsRoot =
      storagePath.slice(0, storagePath.lastIndexOf("/")) + "/attachments/";
    assert(
      localPath.startsWith(attachmentsRoot),
      "default attachment must use the same independently checked private inbox root",
    );
    await client.attachments.deleteLocal(pending.remoteAttachment);
    evidence.push(
      "raw OS sandbox root matches native default database and attachment paths",
    );
    const dm = await client.conversations.createDm(setup.peerInboxId);
    equal(
      await dm.peerInboxId(),
      setup.peerInboxId,
      "DM peer identity must survive JSI",
    );
    const conversationId = dm.id;
    const outboundMessageId = await dm.sendText(setup.outboundText);
    const received = await control.call<{ messageId: string }>(
      "receive-and-reply",
      {
        conversationId,
        messageId: outboundMessageId,
        senderInboxId: inboxId,
      },
    );
    evidence.push("Node sibling received exact RN DM id, sender and text");
    await dm.sync();
    const inbound = await client.conversations.getMessageById(
      received.messageId,
    );
    assert(inbound, "RN must read the sibling reply");
    equal(
      inbound.senderInboxId,
      setup.peerInboxId,
      "reply must retain sibling sender",
    );
    assert(inbound.content.kind === "text", "reply must decode as text");
    equal(
      inbound.content.value,
      setup.inboundText,
      "reply must retain exact sibling text",
    );
    evidence.push("RN received exact Node sibling reply");

    await client.end();
    try {
      await client.conversations.list();
      throw new Error("ended client accepted an operation");
    } catch (error) {
      assert(
        error instanceof XmtpError.ClientClosed,
        "ended client must reject with ClientClosed",
      );
    }
    await client.end();
    evidence.push("typed closed-client error and idempotent end");

    reopened = await Client.create(signer, {
      ...options,
      storage: {
        ...options.storage,
        location: {
          dbPath: storagePath,
          attachmentsDir: `${storagePath}.attachments`,
        },
      },
    });
    equal(reopened.inboxId, inboxId, "reopen must retain inbox identity");
    equal(
      reopened.installationId,
      installationId,
      "reopen must retain installation identity",
    );
    equal(
      reopened.storagePath,
      storagePath,
      "reopen must use the same private database",
    );
    equal(
      (await reopened.conversations.getMessageById(outboundMessageId))?.id,
      outboundMessageId,
      "reopen must retain outgoing history",
    );
    equal(
      (await reopened.conversations.getMessageById(received.messageId))?.id,
      received.messageId,
      "reopen must retain incoming history",
    );
    evidence.push(
      "encrypted private storage reopen retains identity and both DM messages",
    );
    // One stream read keeps a JSI async iterator on every device run.
    const conversation = await reopened.conversations.getById(conversationId);
    assert(conversation, "reopen must retain the DM");
    const reader = conversation.streamMessages();
    try {
      const known = new Set([outboundMessageId, received.messageId]);
      for (;;) {
        const item = await within(reader.next(), "JSI message stream read");
        assert(!item.done, "message stream must deliver the retained DM");
        if (!known.has(item.value.id)) continue;
        assert(
          item.value.content.kind === "text",
          "streamed DM message must decode as text",
        );
        break;
      }
    } finally {
      await reader.end();
    }
    evidence.push("message stream next() delivers a retained DM message");
    phases.push("foundation");
    if (conformance) {
      evidence.push(...contentVectorJourney());
      phases.push("content");
      evidence.push(...(await boundaryJourney(reopened, signer, options)));
      phases.push("boundary");
      evidence.push(...(await listenerJourney(reopened)));
      phases.push("listener");
      assert(
        typeof XmtpProvider === "function",
        "installed /react entry must resolve its provider",
      );
      evidence.push("installed /react entry resolves");
      phases.push("react");
    }
    return {
      journeys: setup.journeys,
      assertions: evidence,
      phases,
      inboxId,
      conversationId,
      outboundMessageId,
      inboundMessageId: received.messageId,
    };
  } finally {
    await reopened?.end();
    await client?.end();
  }
}
