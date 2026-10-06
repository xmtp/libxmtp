// Browser runtime behavior that only the package can show: callbacks that
// call back into the worker, the JavaScript stream adapter over the worker,
// JavaScript codec failures, and the OPFS storage rules.
import {
  Backend,
  Client,
  MessageStream,
  Storage,
  XmtpError,
  type ClientOptions,
  type ContentCodec,
  type EncodedContent,
  type StreamCloseReason,
} from "@xmtp/browser-sdk";
import { expect, test } from "vitest";

import { backend, create, options, signer } from "./helpers";

async function rejection(action: () => Promise<unknown>): Promise<unknown> {
  let result: Promise<unknown>;
  try {
    result = action();
  } catch (error) {
    throw new Error("The call threw synchronously", { cause: error });
  }
  expect(result).toBeInstanceOf(Promise);
  return result.then(
    () => {
      throw new Error("The call did not fail");
    },
    (reason: unknown) => reason,
  );
}

function closed(error: unknown): void {
  expect(error).toBeInstanceOf(XmtpError.ClientClosed);
  expect((error as XmtpError).details).toMatchObject({
    code: "ClientClosed",
    category: "lifecycle",
    retryable: false,
  });
}

test("a signer callback calls the worker while Rust waits, and an ended client rejects calls", async () => {
  const owner = signer();
  const sign = owner.sign.bind(owner);
  let connected: Backend | undefined;
  owner.sign = async (request) => {
    connected ??= await Backend.connect(backend);
    return sign(request);
  };
  const client = await create(owner);
  expect(connected).toBeInstanceOf(Backend);
  const identity = await owner.identity();
  expect(await Client.canMessage([identity], connected!)).toStrictEqual(
    new Map([[`ethereum:${identity.identifier}`, true]]),
  );

  const group = await client.conversations.createGroup([]);
  const message = await client.conversations.getMessageById(
    await group.sendText("held across end"),
  );
  if (!message) throw new Error("The sent message is missing");
  await client.end();
  closed(await rejection(() => client.conversations.listGroups(undefined)));
  // A held Message keeps its fields; its actions return rejected promises.
  expect(message.content).toStrictEqual({
    kind: "text",
    value: "held across end",
  });
  for (const action of [
    () => message.refresh(),
    () => message.delete(),
    () => message.deleteLocally(),
    () => message.reply("closed"),
    () => message.conversation(),
  ])
    closed(await rejection(action));
});

// The worker holds the SignatureRequest; its sign call takes the
// main-thread signer as an argument and calls back into it.
test("a signature request signs with a main-thread signer through the worker", async () => {
  const owner = signer();
  const client = await create(owner, { registration: { auto: false } });
  expect(await client.isRegistered()).toBe(false);
  const request = await client.unsafeCreateInboxSignatureRequest();
  if (!request) throw new Error("An unregistered client has no request");
  expect((await request.signatureText()).length).toBeGreaterThan(0);
  await request.sign(owner);
  await client.unsafeApplySignatureRequest(request);
  expect(await client.isRegistered()).toBe(true);
});

test("a stream replays an unacknowledged message, cancels an idle read, and reports its close", async () => {
  const client = await create();
  const group = await client.conversations.createGroup([]);
  const reader = await group.messageReader();
  const read = reader.next();
  const firstId = await group.sendText("first");
  expect((await read)?.id).toBe(firstId);
  await reader.end();

  const reasons: StreamCloseReason["kind"][] = [];
  const states: string[] = [];
  const stream = MessageStream.openGroup(client, group, undefined, {
    onClose: (reason) => reasons.push(reason.kind),
    onConnectionStateChange: (_previous, current) => states.push(current),
  });
  // Ending the reader did not acknowledge its last message.
  expect((await stream.next()).value?.id).toBe(firstId);
  await expect.poll(() => states.length).toBeGreaterThan(0);
  expect(["connected", "connecting"]).toContain(states[0]);
  await stream.return();
  expect(reasons).toStrictEqual(["closed"]);

  const replay = MessageStream.openGroup(client, group);
  expect((await replay.next()).value?.id).toBe(firstId);
  const pending = replay.next();
  const nextId = await group.sendText("next");
  expect((await pending).value?.id).toBe(nextId);
  const idle = replay.next();
  await replay.return();
  expect(await idle).toStrictEqual({ done: true, value: undefined });
});

// verifies: PROC-045, CTYPE-009
test("hostile codec failures stay typed and the stream delivers the next item", async () => {
  const type = {
    authorityId: "tests.xmtp.org",
    typeId: "hostile",
    versionMajor: 1,
    versionMinor: 0,
  };
  const encode = (value: string): EncodedContent => ({
    type,
    parameters: new Map(),
    content: new TextEncoder().encode(value),
  });
  const codec: ContentCodec<string> = {
    type,
    encode,
    decode(encoded) {
      const value = new TextDecoder().decode(encoded.content);
      if (value === "null prototype") throw Object.create(null);
      if (value === "throwing toString")
        // oxlint-disable-next-line typescript/only-throw-error -- Check a hostile codec failure value.
        throw {
          toString() {
            throw new Error("diagnostic failed");
          },
        };
      if (value === "error") throw new Error("bad custom payload");
      return value;
    },
  };
  const client = await create(signer(), { codecs: [codec] });
  const group = await client.conversations.createGroup([]);
  const stream = MessageStream.openGroup(client, group);
  try {
    for (const [value, message] of [
      ["error", "bad custom payload"],
      ["null prototype", "custom content codec failed"],
      ["throwing toString", "custom content codec failed"],
    ]) {
      const failedId = await group.send(encode(value));
      const failed = (await stream.next()).value;
      expect(failed?.id).toBe(failedId);
      expect(failed?.content).toMatchObject({
        kind: "custom",
        error: {
          code: "CodecDecodeFailed",
          category: "callback",
          retryable: false,
        },
      });
      if (failed?.content.kind !== "custom") throw new Error("Not custom");
      expect(failed.content.value).toBeUndefined();
      expect(failed.content.error?.message).toContain(message);
      expect(failed.content.rawBytes.byteLength).toBeGreaterThan(0);

      const goodId = await group.send(encode(`after ${value}`));
      const good = (await stream.next()).value;
      expect(good?.id).toBe(goodId);
      expect(good?.content).toMatchObject({
        kind: "custom",
        value: `after ${value}`,
      });
    }
  } finally {
    await stream.end();
  }
});

// verifies: STORE-005
test("default storage lives under the xmtp-sdk OPFS directory and reopens there", async () => {
  const owner = signer();
  const settings: Partial<ClientOptions> = { storage: { location: "default" } };
  const first = await create(owner, settings);
  const path = first.storagePath;
  expect(path?.startsWith("xmtp-sdk/")).toBe(true);
  await first.end();
  const reopened = await create(owner, settings);
  try {
    expect(reopened.storagePath).toBe(path);
  } finally {
    await reopened.end();
    const admin = await Storage.admin();
    try {
      for (const file of await admin.listFiles())
        if (file.replace(/^\/+/, "") === path) await admin.deleteFile(file);
    } finally {
      await admin.end();
    }
  }
});

test("browser storage refuses an encryption key before it opens a database", async () => {
  const error = await Client.create(signer(), {
    ...options,
    storage: { location: "inMemory", encryptionKey: new Uint8Array(32) },
  } as ClientOptions).then(
    async (client) => {
      await client.end();
      throw new Error("The browser accepted an encryption key");
    },
    (reason: unknown) => reason,
  );
  expect(error).toBeInstanceOf(XmtpError.InvalidInput);
  expect((error as XmtpError).details.category).toBe("input");
});
