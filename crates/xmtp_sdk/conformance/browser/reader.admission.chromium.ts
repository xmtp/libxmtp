import { expect } from "vitest";

// @ts-ignore The fixture uses the published viem JavaScript build.
import { generatePrivateKey } from "../../../../sdks/browser/node_modules/viem/_esm/accounts/generatePrivateKey.js";
// @ts-ignore The fixture uses the published viem JavaScript build.
import { privateKeyToAccount } from "../../../../sdks/browser/node_modules/viem/_esm/accounts/privateKeyToAccount.js";
// @ts-ignore The fixture uses the published viem JavaScript build.
import { toBytes } from "../../../../sdks/browser/node_modules/viem/_esm/utils/encoding/toBytes.js";
import {
  Message,
  MessageStream,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/binding";
import {
  CONTRACT_HASH,
  PROTOCOL_VERSION,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/contract.gen";
// Transport tests use the worker proxy Client with their own session.
import { Client } from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/proxy.gen";
import { MainSession } from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/runtime/bridge/main/session";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/runtime/bridge/wire";
import type { StreamCloseReason } from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/runtime/streams/reader";
import * as B from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/xmtp_sdk";
import { encodeText } from "../../../../target/sdk-generated/typescript-pure/binding";

function latch() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

export type AdmissionCase =
  | "large-cursor"
  | "restored-peer"
  | "admitted"
  | "cancel"
  | "owner-end"
  | "overlap-end"
  | "end-fails"
  | "callback-throw"
  | "callback-reject";

// verifies: PROC-025, PROC-052, PROC-046, PROC-050
export async function checkWorkerAdmission(
  backendURL: string,
  mode: AdmissionCase,
): Promise<void> {
  const worker = new Worker(
    new URL("./reader.admission.worker.ts", import.meta.url),
    { type: "module" },
  );
  const returns = new Map<number, ReturnType<typeof latch>>();
  let lastNext = 0;
  const endpoint: WireEndpoint = {
    postMessage(message, transfer) {
      if (message.t === "call" && message.key === "MessageReader.next") {
        lastNext = message.id;
        returns.set(message.id, latch());
      }
      worker.postMessage(message, { transfer });
    },
    onMessage(handler) {
      worker.addEventListener("message", (event: MessageEvent<WireMessage>) => {
        handler(event.data);
        if (event.data.t === "return" || event.data.t === "error")
          returns.get(event.data.id)?.resolve();
      });
    },
    onExit(handler) {
      worker.addEventListener("error", handler);
    },
    terminate() {
      worker.terminate();
    },
  };
  const session = new MainSession(endpoint, PROTOCOL_VERSION, CONTRACT_HASH);
  const account = privateKeyToAccount(generatePrivateKey());
  const identity = {
    identifier: account.address.toLowerCase(),
    kind: B.PublicIdentityKind.Ethereum,
  };
  const path = `reader-admission-${crypto.randomUUID()}.db`;
  const options = {
    backend: B.BackendSource.Options.new({
      options: {
        url: backendURL,
        appVersion: undefined,
        credential: undefined,
        credentials: undefined,
      },
    }),
    storage: {
      location: B.StorageLocation.Explicit.new({
        dbPath: path,
        attachmentsDir: `${path}-attachments`,
      }),
      label: path,
      pool: undefined,
      singleConnection: false,
    },
    deviceSync: false,
    allowOffline: false,
    registration: { auto: true, nonce: undefined },
    forkRecovery: undefined,
    workers: undefined,
  };
  let client: Client | undefined;
  let release: number | undefined;
  try {
    await session.ready();
    client = await Client.create(
      session,
      {
        async identity() {
          return identity;
        },
        async kind() {
          return B.SignerKind.Eoa.new();
        },
        async sign(request: { text: string }) {
          return B.Signature.Ecdsa.new(
            Uint8Array.from(
              toBytes(await account.signMessage({ message: request.text })),
            ).buffer,
          );
        },
      },
      options,
    );
    const inbox = client.inboxId();
    if (mode === "restored-peer") {
      const makePeer = async () => {
        const account = privateKeyToAccount(generatePrivateKey());
        const peerPath = `peer-${crypto.randomUUID()}.db`;
        return Client.create(
          session,
          {
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
              return B.Signature.Ecdsa.new(
                Uint8Array.from(
                  toBytes(await account.signMessage({ message: request.text })),
                ).buffer,
              );
            },
          },
          {
            ...options,
            storage: {
              ...options.storage,
              location: B.StorageLocation.Explicit.new({
                dbPath: peerPath,
                attachmentsDir: `${peerPath}-attachments`,
              }),
              label: peerPath,
            },
          },
        );
      };
      const b = await makePeer();
      const c = await makePeer();
      try {
        const dm = await client
          .conversations()
          .createDm(b.inboxId(), undefined);
        const other = await b
          .conversations()
          .createDm(client.inboxId(), undefined);
        expect(dm.id()).not.toBe(other.id());
        expect(await dm.peerInboxId()).toBe(b.inboxId());
        expect(await other.peerInboxId()).toBe(client.inboxId());
        await client.conversations().syncAll(undefined);
        const id = await dm.sendText("foreign restored DM", undefined);
        const key = new Uint8Array(32).fill(9).buffer;
        const archive = await client
          .archives()
          .exportToBytes(
            key,
            B.ArchiveOptions.create({ elements: [B.ArchiveElement.Messages] }),
          );
        await c.archives().importFromBytes(archive, key);
        const conversation = await c.conversations().getById(dm.id());
        if (conversation?.tag !== B.Conversation_Tags.Dm)
          throw new Error("DM missing");
        const restored = conversation.inner.dm;
        // The binding reports an absent peer as undefined; the public layer
        // returns null.
        expect(await restored.peerInboxId()).toBeUndefined();
        const listed = await c
          .conversations()
          .listDms(
            B.ListConversationsOptions.create({ includeDuplicateDms: true }),
          );
        expect(listed).toHaveLength(2);
        for (const item of listed)
          expect(await item.peerInboxId()).toBeUndefined();
        const duplicates = await restored.duplicateDms();
        expect(duplicates).toHaveLength(1);
        expect(await duplicates[0].peerInboxId()).toBeUndefined();
        const cursor = (await restored.messages(undefined)).find(
          (message) => message.id === id,
        )!.deliveryCursor;
        expect(cursor).toEqual(expect.any(String));
        const stream = MessageStream.openDm(c, restored, {
          from: await c.conversations().beginningDeliveryCursor(),
        });
        const first = (await stream.next()).value;
        expect(first?.id).toBe(id);
        expect(first?.deliveryCursor).toBe(cursor);
        await stream.end();
        const reader = await restored.messageReader();
        expect((await reader.next())?.deliveryCursor).toBe(cursor);
        await reader.end();
        await expect(
          restored.messageReader({ from: "invalid" }),
        ).rejects.toBeInstanceOf(B.XmtpError.InvalidCursor);
        await expect(
          restored.messageReader({
            from: await client.conversations().beginningDeliveryCursor(),
          }),
        ).rejects.toBeInstanceOf(B.XmtpError.ForeignCursor);
      } finally {
        await c.end();
        await b.end();
      }
      return;
    }
    if (mode === "large-cursor") {
      await client.conversations().sdkConformanceSeedDeliveryCursor();
      const group = await client.conversations().createGroup([], undefined);
      const groupId = group.id();
      const beginning = await client.conversations().beginningDeliveryCursor();
      const firstId = await group.sendText("large A", undefined);
      const first = (await group.messages(undefined)).find(
        (message) => message.id === firstId,
      )!;
      if (!(first instanceof Message))
        throw new Error("history did not lift Message");
      const cursor = first.deliveryCursor!;
      const sequence = (value: string) => {
        expect(value.startsWith("dc1_")).toBe(true);
        const bytes = Uint8Array.from(
          atob(value.slice(4).replaceAll("-", "+").replaceAll("_", "/")),
          (byte) => byte.charCodeAt(0),
        );
        expect(bytes.byteLength).toBe(24);
        return new DataView(bytes.buffer).getBigUint64(16);
      };
      expect(sequence(cursor)).toBe(9007199254740993n);
      expect(
        (await client.conversations().getMessageById(firstId))?.deliveryCursor,
      ).toBe(cursor);
      expect((await first.refresh())?.deliveryCursor).toBe(cursor);
      const all = MessageStream.open(client, {
        from: beginning,
        consentStates: undefined,
        conversationKind: B.ConversationKind.Group,
      });
      expect((await all.next()).value?.deliveryCursor).toBe(cursor);
      await all.end();
      const named = MessageStream.openGroup(client, group);
      expect((await named.next()).value?.deliveryCursor).toBe(cursor);
      await named.end();
      const secondId = await group.sendText("large B", undefined);
      const resume = await group.messageReader({ from: cursor });
      const second = await resume.next();
      expect(second?.id).toBe(secondId);
      expect(sequence(second!.deliveryCursor!)).toBe(9007199254740994n);
      await resume.end();
      const encoded = encodeText("reply");
      const replyId = await group.sendReply(
        firstId,
        undefined,
        encoded,
        undefined,
      );
      const reply = await client.conversations().getMessageById(replyId);
      if (!(reply instanceof Message))
        throw new Error("lookup did not lift Message");
      expect((await reply.parent())?.deliveryCursor).toBe(cursor);
      const preparedId = await group.prepareMessage(encoded, undefined);
      expect(
        (await client.conversations().getMessageById(preparedId))
          ?.deliveryCursor,
      ).toBe(null);
      await group.publishMessage(preparedId);
      expect(
        (await client.conversations().getMessageById(preparedId))
          ?.deliveryCursor,
      ).toEqual(expect.any(String));
      await client.end();
      client = await Client.build(session, identity, options, inbox);
      const restored = await client.conversations().getById(groupId);
      if (restored?.tag !== B.Conversation_Tags.Group)
        throw new Error("group missing");
      const replay = MessageStream.openGroup(client, restored.inner.group, {
        from: cursor,
      });
      const repeated = (await replay.next()).value;
      expect(repeated?.id).toBe(secondId);
      expect(repeated?.deliveryCursor).toBe(second?.deliveryCursor);
      await replay.end();
      return;
    }
    let group = await client.conversations().createGroup([], undefined);
    const groupId = group.id();
    const ids: string[] = [];
    const cursors: string[] = [];
    for (const text of ["A", "B", "C"]) {
      const id = await group.sendText(text, undefined);
      ids.push(id);
      const message = await client.conversations().getMessageById(id);
      expect(message?.deliveryCursor).toEqual(expect.any(String));
      cursors.push(message!.deliveryCursor!);
    }
    const closed: StreamCloseReason[] = [];
    const abort = new AbortController();
    const stream =
      mode === "admitted"
        ? MessageStream.open(client, undefined, {
            signal: abort.signal,
            onClose: (reason) => closed.push(reason),
          })
        : MessageStream.openGroup(client, group, undefined, {
            signal: abort.signal,
            onClose: (reason) => closed.push(reason),
          });
    let callbackError: Error | undefined;
    if (mode.startsWith("callback")) {
      callbackError = new Error(mode);
      let calls = 0;
      await expect(
        stream.onValue((message) => {
          expect(message.id).toBe(ids[calls]);
          expect(message.deliveryCursor).toBe(cursors[calls]);
          calls++;
          if (calls === 2) {
            if (mode === "callback-throw") throw callbackError;
            return Promise.reject(callbackError);
          }
        }),
      ).rejects.toBe(callbackError);
      expect(calls).toBe(2);
    } else {
      const a = await stream.next();
      expect(a.done).toBe(false);
      if (!a.done) expect(a.value.id).toBe(ids[0]);
      release = (await session.call("__f3ArmNext", [])) as number;
      const pending = stream.next();
      const settled = pending.then(
        (value) => ({ value }),
        (error: unknown) => ({ error }),
      );
      let delivered = false;
      void settled.then(() => {
        delivered = true;
      });
      expect(await session.call("__f3WaitHeld", [])).toBe(release);
      const late = returns.get(lastNext)!.promise;
      const bytes = Uint8Array.from(
        atob(cursors[0].slice(4).replaceAll("-", "+").replaceAll("_", "/")),
        (byte) => byte.charCodeAt(0),
      );
      const acknowledgedA = new DataView(bytes.buffer)
        .getBigUint64(16)
        .toString();
      expect(
        await client.conversations().sdkConformanceDeliveryPosition(groupId),
      ).toBe(acknowledgedA);
      if (mode === "admitted") {
        await group.updateConsentState(B.ConsentState.Denied);
      } else if (mode === "end-fails") {
        // The admitted reply arrives while Client.end runs, and then the end
        // fails. The client stays open and the held value reaches the app.
        await session.call("__f3HoldEnd", []);
        const ending = client.end().then(
          () => undefined,
          (error: unknown) => error,
        );
        await session.call("__f3WaitEnd", []);
        await session.call("__f3Release", [release]);
        release = undefined;
        await late;
        await new Promise<void>((resolve) => setTimeout(resolve, 20));
        expect(delivered).toBe(false);
        await session.call("__f3FailEnd", []);
        expect(await ending).toBeDefined();
        const value = await pending;
        expect(!value.done && value.value.id).toBe(ids[1]);
        await stream.end();
      } else if (mode === "cancel") {
        abort.abort();
        expect(await session.call("__f3WaitAbort", [])).toBe(release);
        expect(await pending).toEqual({ done: true, value: undefined });
        await stream.end();
      } else if (mode === "overlap-end") {
        // A second read while the first value is in transit must reject.
        // It must not acknowledge the first value before the end below.
        // Catch the error before the test runner checks for unhandled errors.
        const second = stream.next().then(
          () => ({ accepted: true as const }),
          (error: unknown) => ({ accepted: false as const, error }),
        );
        const secondResult = await second;
        if (secondResult.accepted)
          throw new Error("overlapping read succeeded");
        expect(secondResult.error).toBeInstanceOf(Error);
        expect((secondResult.error as Error).message).toBe(
          "reader iterator read is active",
        );
        await new Promise<void>((resolve) => setTimeout(resolve, 50));
        expect(await session.call("__f3Counts", [])).toMatchObject({
          nextCalls: 2,
        });
        await client.end();
        client = undefined;
        await stream.end();
        expect(await pending).toEqual({ done: true, value: undefined });
        const check = await Client.build(session, identity, options, inbox);
        expect(
          await check.conversations().sdkConformanceDeliveryPosition(groupId),
        ).toBe(acknowledgedA);
        await check.end();
      } else {
        await client.end();
        client = undefined;
        await stream.end();
        expect(await pending).toEqual({ done: true, value: undefined });
      }
      if (mode !== "admitted")
        expect(await session.call("__f3Counts", [])).toEqual({
          nextCalls: 2,
          endCalls: mode === "owner-end" || mode === "overlap-end" ? 0 : 1,
          endCompletions:
            mode === "owner-end" || mode === "overlap-end" ? 0 : 1,
        });
      if (client)
        expect(
          await client.conversations().sdkConformanceDeliveryPosition(groupId),
        ).toBe(acknowledgedA);
      if (release !== undefined) await session.call("__f3Release", [release]);
      release = undefined;
      await late;
      const result = await settled;
      if (mode === "admitted" || mode === "end-fails") {
        expect(
          "value" in result && !result.value.done && result.value.value.id,
        ).toBe(ids[1]);
        if ("value" in result && !result.value.done)
          expect(result.value.value.deliveryCursor).toBe(cursors[1]);
      }
      await stream.end();
    }
    expect(await session.call("__f3Counts", [])).toEqual({
      nextCalls: 2,
      endCalls: mode === "owner-end" || mode === "overlap-end" ? 0 : 1,
      endCompletions: mode === "owner-end" || mode === "overlap-end" ? 0 : 1,
    });
    await client?.end();
    client = undefined;
    const reopen = async (): Promise<B.GroupLike> => {
      await client?.end();
      client = await Client.build(session, identity, options, inbox);
      const conversation = await client.conversations().getById(groupId);
      if (conversation?.tag !== B.Conversation_Tags.Group)
        throw new Error("persisted group missing");
      return conversation.inner.group;
    };
    // Default delivery is the durable progress oracle. Replay alone cannot prove D.
    for (let attempt = 0; attempt < 2; attempt++) {
      group = await reopen();
      const reader = await group.messageReader();
      const b = await reader.next();
      expect(b?.id).toBe(ids[1]);
      expect(b?.deliveryCursor).toBe(cursors[1]);
      if (attempt === 1) {
        const c = await reader.next();
        expect(c?.id).toBe(ids[2]);
        expect(c?.deliveryCursor).toBe(cursors[2]);
      }
      await reader.end();
    }
    group = await reopen();
    const reader = await group.messageReader();
    expect((await reader.next())?.id).toBe(ids[2]);
    await reader.end();
    const replay = await group.messageReader({ from: cursors[0] });
    const b = await replay.next();
    expect(b?.id).toBe(ids[1]);
    expect(b?.deliveryCursor).toBe(cursors[1]);
    await replay.end();
    expect(closed).toEqual([
      callbackError
        ? { kind: "failed", error: callbackError }
        : { kind: "closed" },
    ]);
  } finally {
    if (release !== undefined)
      await session.call("__f3Release", [release]).catch(() => {});
    await client?.end().catch(() => {});
    worker.terminate();
  }
}
