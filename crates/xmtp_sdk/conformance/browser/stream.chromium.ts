import {
  CONTRACT_HASH,
  PROTOCOL_VERSION,
} from "../../../../target/sdk-generated/typescript-wasm/contract.gen";
import * as sdk from "../../../../target/sdk-generated/typescript-wasm/index";
import { MainSession } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/session";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire";
import { checkReaderLoopExit } from "../ts/reader-loop-exit";
import { create, options, signer } from "./suite-support";

function live(state: sdk.ConnectionState, label: string): void {
  if (state !== "connected" && state !== "connecting")
    throw new Error(`${label} connection state was ${state}`);
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

async function createClient(
  session: MainSession,
  backendURL: string,
): Promise<sdk.Client> {
  const path = `stream-${crypto.randomUUID()}.db`;
  return (await create(session, signer(session), options(path, backendURL)))
    .client;
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
  const clients: sdk.Client[] = [];
  let step = "create clients";
  try {
    await session.ready();
    const alice = await createClient(session, backendURL);
    clients.push(alice);
    const bob = await createClient(session, backendURL);
    clients.push(bob);

    step = "public reader loop exits";
    await checkReaderLoopExit(
      async () => {
        const group = await alice.conversations.createGroup([]);
        return { group, id: await group.sendText("held loop item") };
      },
      (group, options) =>
        sdk.MessageStream.openGroup(alice, group, undefined, options),
    );

    step = "create group";
    const group = await alice.conversations.createGroup([bob.inboxId]);
    step = "open reader";
    const reader = await group.messageReader();
    live(await reader.connectionState(), "reader");
    step = "peer sync";
    await bob.conversations.sync();
    const peer = await bob.conversations.getById(group.id);
    if (!(peer instanceof sdk.Group)) throw new Error("peer has no group");
    step = "reader delivery";
    const firstId = await peer.sendText("browser reader");
    // The reader first yields the group update that added the peer.
    let read = await withTimeout(reader.next(), "reader delivery");
    if (read?.content.kind === "groupUpdated")
      read = await withTimeout(reader.next(), "reader delivery");
    if (read?.id !== firstId)
      throw new Error(
        `reader yielded ${String(read?.content.kind)}, not the message`,
      );
    step = "reader end";
    await reader.end();
    if ((await reader.connectionState()) !== "closed")
      throw new Error("ended reader is not closed");

    // One reader owns a conversation at a time. The stream opens the next one
    // and gets the unacknowledged message again.
    step = "stream redelivery";
    const states: sdk.ConnectionState[] = [];
    const stream = sdk.MessageStream.openGroup(alice, group, undefined, {
      onConnectionStateChange: (_previous, current) => states.push(current),
    });
    const replayed = await withTimeout(stream.next(), "stream redelivery");
    if (replayed.done || replayed.value.id !== firstId)
      throw new Error("stream did not redeliver the message");
    step = "stream delivery";
    const secondId = await peer.sendText("browser stream");
    const streamed = await withTimeout(stream.next(), "stream delivery");
    if (streamed.done || streamed.value.id !== secondId)
      throw new Error("stream missed the message");
    if (!(streamed.value instanceof sdk.Message))
      throw new Error("stream yielded no public Message");
    if (streamed.value.senderInboxId !== bob.inboxId)
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
