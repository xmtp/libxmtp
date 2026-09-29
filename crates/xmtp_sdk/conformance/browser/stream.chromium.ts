// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { generatePrivateKey } from "../../../../sdks/browser/node_modules/viem/_esm/accounts/generatePrivateKey.js";
// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { privateKeyToAccount } from "../../../../sdks/browser/node_modules/viem/_esm/accounts/privateKeyToAccount.js";
// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { toBytes } from "../../../../sdks/browser/node_modules/viem/_esm/utils/encoding/toBytes.js";
import {
  CONTRACT_HASH,
  PROTOCOL_VERSION,
} from "../../../../target/sdk-generated/typescript-wasm/contract.gen";
import { MessageStream } from "../../../../target/sdk-generated/typescript-wasm/index";
// Transport tests use the worker proxy Client with their own session.
import { Client } from "../../../../target/sdk-generated/typescript-wasm/proxy.gen";
import { MainSession } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/session";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire";
import * as B from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk";

function live(state: B.ConnectionState, label: string): void {
  if (
    state !== B.ConnectionState.Connected &&
    state !== B.ConnectionState.Connecting
  )
    throw new Error(
      `${label} connection state was ${B.ConnectionState[state]}`,
    );
}

function withTimeout<T>(value: Promise<T>, label: string): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  return Promise.race([
    value,
    new Promise<never>((_, reject) => {
      timer = setTimeout(() => reject(new Error(`${label} timed out`)), 20_000);
    }),
  ]).finally(() => clearTimeout(timer));
}

function createClient(
  session: MainSession,
  backendURL: string,
): Promise<Client> {
  const account = privateKeyToAccount(generatePrivateKey());
  const path = `stream-${crypto.randomUUID()}.db`;
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
        const signed = await account.signMessage({ message: request.text });
        return B.Signature.Ecdsa.new(Uint8Array.from(toBytes(signed)).buffer);
      },
    },
    {
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
      registration: { auto: true, nonce: undefined },
      forkRecovery: undefined,
      workers: undefined,
    },
  );
}

// A peer message reaches a MessageReader and a MessageStream through the real
// browser Worker bridge, and both close cleanly.
export async function checkMessageStream(backendURL: string): Promise<void> {
  // The deleted-message worker is a plain generated worker host.
  const worker = new Worker(
    new URL("./message.deleted.worker.ts", import.meta.url),
    {
      type: "module",
    },
  );
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
  const clients: Client[] = [];
  let step = "create clients";
  try {
    await session.ready();
    const alice = await createClient(session, backendURL);
    clients.push(alice);
    const bob = await createClient(session, backendURL);
    clients.push(bob);

    step = "create group";
    const group = await alice
      .conversations()
      .createGroup([bob.inboxId()], undefined);
    step = "open reader";
    const reader = await group.messageReader();
    live(await reader.connectionState(), "reader");
    step = "peer sync";
    await bob.conversations().sync();
    const peer = await bob.conversations().getById(group.id());
    if (peer?.tag !== B.Conversation_Tags.Group)
      throw new Error("peer has no group");
    step = "reader delivery";
    const firstId = await peer.inner.group.sendText(
      "browser reader",
      undefined,
    );
    // The reader first yields the group update that added the peer.
    let read = await withTimeout(reader.next(), "reader delivery");
    if (read?.content.tag === B.MessageContent_Tags.GroupUpdated)
      read = await withTimeout(reader.next(), "reader delivery");
    if (read?.id.toString() !== firstId.toString())
      throw new Error(
        `reader yielded ${String(read?.content.tag)}, not the message`,
      );
    step = "reader end";
    await reader.end();
    if ((await reader.connectionState()) !== B.ConnectionState.Closed)
      throw new Error("ended reader is not closed");

    // One reader owns a conversation at a time. The stream opens the next one
    // and gets the unacknowledged message again.
    step = "stream redelivery";
    const states: B.ConnectionState[] = [];
    const stream = MessageStream.openGroup(alice, group, undefined, {
      onConnectionStateChange: (_previous, current) => states.push(current),
    });
    const replayed = await withTimeout(stream.next(), "stream redelivery");
    if (replayed.done || replayed.value.id.toString() !== firstId.toString())
      throw new Error("stream did not redeliver the message");
    step = "stream delivery";
    const secondId = await peer.inner.group.sendText(
      "browser stream",
      undefined,
    );
    const streamed = await withTimeout(stream.next(), "stream delivery");
    if (streamed.done || streamed.value.id.toString() !== secondId.toString())
      throw new Error("stream missed the message");
    if (streamed.value.senderInboxId.toString() !== bob.inboxId().toString())
      throw new Error("stream message has the wrong sender");
    if (states.length === 0)
      throw new Error("stream reported no connection state");
    live(states[0], "stream");

    step = "stream return";
    const idle = stream.next();
    setTimeout(() => void stream.return(), 50);
    if (!(await withTimeout(idle, "idle stream read")).done)
      throw new Error("return did not end the idle read");
    if (!(await stream.next()).done) throw new Error("returned stream yielded");
  } catch (error) {
    throw new Error(`${step}: ${String(error)}`);
  } finally {
    for (const client of clients) await client.end();
    worker.terminate();
  }
}
