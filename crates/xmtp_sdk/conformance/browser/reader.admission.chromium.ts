import { expect } from "vitest";

// @ts-ignore The fixture uses the published viem JavaScript build.
import { generatePrivateKey } from "../../../../sdks/browser/node_modules/viem/_esm/accounts/generatePrivateKey.js";
// @ts-ignore The fixture uses the published viem JavaScript build.
import { privateKeyToAccount } from "../../../../sdks/browser/node_modules/viem/_esm/accounts/privateKeyToAccount.js";
// @ts-ignore The fixture uses the published viem JavaScript build.
import { toBytes } from "../../../../sdks/browser/node_modules/viem/_esm/utils/encoding/toBytes.js";
import {
  CONTRACT_HASH,
  PROTOCOL_VERSION,
} from "../../../../target/sdk-generated/typescript-wasm/contract.gen";
import {
  Client,
  MessageStream,
} from "../../../../target/sdk-generated/typescript-wasm/index";
import { MainSession } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/session";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire";
import type { StreamCloseReason } from "../../../../target/sdk-generated/typescript-wasm/runtime/streams/reader";
import * as B from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk";

function latch() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

export type AdmissionCase =
  | "admitted"
  | "cancel"
  | "owner-end"
  | "callback-throw"
  | "callback-reject";

// verifies: PROC-025, PROC-028, PROC-046, PROC-050
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
      location: B.StorageLocation.Path.new(path),
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
      expect(await session.call("__f3WaitHeld", [])).toBe(release);
      const late = returns.get(lastNext)!.promise;
      if (mode === "admitted") {
        await group.updateConsentState(B.ConsentState.Denied);
      } else if (mode === "cancel") {
        abort.abort();
        expect(await session.call("__f3WaitAbort", [])).toBe(release);
        expect(await pending).toEqual({ done: true, value: undefined });
        await stream.end();
      } else {
        await client.end();
        client = undefined;
        await stream.end();
        expect(await pending).toEqual({ done: true, value: undefined });
      }
      if (mode !== "admitted")
        expect(await session.call("__f3Counts", [])).toEqual({
          nextCalls: 2,
          endCalls: mode === "owner-end" ? 0 : 1,
          endCompletions: mode === "owner-end" ? 0 : 1,
        });
      await session.call("__f3Release", [release]);
      release = undefined;
      await late;
      const result = await settled;
      if (mode === "admitted") {
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
      endCalls: mode === "owner-end" ? 0 : 1,
      endCompletions: mode === "owner-end" ? 0 : 1,
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
