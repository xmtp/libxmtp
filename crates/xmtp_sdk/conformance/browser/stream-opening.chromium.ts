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
import { create, equal, expect, options, signer } from "./suite-support";

function latch() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

async function within<T>(promise: Promise<T>, label: string): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      promise,
      new Promise<never>((_, reject) => {
        timer = setTimeout(
          () => reject(new Error(`${label} timed out`)),
          20_000,
        );
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}

// Delay actual successful replies. The worker, binding calls, and reply values
// stay unchanged. One connection owns one opening and its cleanup.
function heldOpeningConnection() {
  const worker = new Worker(
    new URL("./message.deleted.worker.ts", import.meta.url),
    { type: "module" },
  );
  const opening = latch();
  const cleanup = latch();
  let armed = false;
  let passThrough = false;
  let openingId: number | undefined;
  let readerHandle: number | undefined;
  let cleanupId: number | undefined;
  let openingReply: WireMessage | undefined;
  let cleanupReply: WireMessage | undefined;
  let receive!: (message: WireMessage) => void;
  let openingCancels = 0;
  let cleanupCalls = 0;
  let cleanupReturns = 0;
  let cleanupCancels = 0;
  const endpoint: WireEndpoint = {
    postMessage(message, transfer) {
      if (message.t === "call") {
        if (armed && message.key === "Group.messageReader") {
          armed = false;
          openingId = message.id;
        }
        if (
          message.key === "MessageReader.end" &&
          message.target?.h === readerHandle
        ) {
          cleanupCalls++;
          cleanupId = message.id;
        }
      } else if (message.t === "cancel") {
        if (message.id === openingId) openingCancels++;
        if (message.id === cleanupId) cleanupCancels++;
      }
      worker.postMessage(message, { transfer });
    },
    onMessage(handler) {
      receive = handler;
      worker.addEventListener("message", (event: MessageEvent<WireMessage>) => {
        const message = event.data;
        if (message.t === "return" && message.id === openingId) {
          const handle = message.value;
          expect(
            handle !== null &&
              typeof handle === "object" &&
              "h" in handle &&
              typeof handle.h === "number",
            "reader opening returned no handle",
          );
          readerHandle = handle.h;
          if (!passThrough) {
            openingReply = message;
            opening.resolve();
            return;
          }
        }
        if (message.t === "return" && message.id === cleanupId) {
          cleanupReturns++;
          if (!passThrough) {
            cleanupReply = message;
            cleanup.resolve();
            return;
          }
        }
        handler(message);
      });
    },
    onExit(handler) {
      worker.addEventListener("error", handler);
    },
    terminate: () => worker.terminate(),
  };
  const releaseOpening = () => {
    const reply = openingReply;
    openingReply = undefined;
    if (reply) receive(reply);
  };
  const releaseCleanup = () => {
    const reply = cleanupReply;
    cleanupReply = undefined;
    if (reply) receive(reply);
  };
  return {
    worker,
    session: new MainSession(endpoint, PROTOCOL_VERSION, CONTRACT_HASH),
    opening: opening.promise,
    cleanup: cleanup.promise,
    arm: () => {
      armed = true;
    },
    releaseOpening,
    releaseCleanup,
    releaseAll: () => {
      passThrough = true;
      releaseOpening();
      releaseCleanup();
    },
    counts: () => ({
      openingCancels,
      cleanupCalls,
      cleanupReturns,
      cleanupCancels,
    }),
  };
}

// verifies: PROC-041, PROC-042, PROC-052
export async function checkLateReaderOpen(backendURL: string): Promise<void> {
  const gate = heldOpeningConnection();
  let client: sdk.Client | undefined;
  let stream: sdk.MessageStream | undefined;
  let replacement: ReturnType<sdk.Group["messageReader"]> | undefined;
  try {
    client = (
      await create(
        gate.session,
        signer(),
        options(`late-reader-${crypto.randomUUID()}.db`, backendURL),
      )
    ).client;
    const group = await client.conversations.createGroup([]);
    const id = await group.sendText("late opening must not acknowledge this");
    const abort = new AbortController();
    const reasons: sdk.StreamCloseReason[] = [];
    const cleanupAtClose: number[] = [];
    gate.arm();
    stream = sdk.MessageStream.openGroup(client, group, undefined, {
      signal: abort.signal,
      onClose: (reason) => {
        reasons.push(reason);
        cleanupAtClose.push(gate.counts().cleanupReturns);
        replacement = group.messageReader();
        void replacement.catch(() => {});
      },
    });
    const read = stream.next();
    void read.catch(() => {});
    await within(gate.opening, "successful reader opening");
    abort.abort();
    let ended = false;
    const closing = stream.end().then(() => {
      ended = true;
    });
    void closing.catch(() => {});
    equal(gate.counts().openingCancels, 1, "opening cancel count");
    equal(ended, false, "end returned before the held opening");
    equal(reasons.length, 0, "onClose ran before the held opening");
    gate.releaseOpening();
    equal(
      await within(
        Promise.race([
          gate.cleanup.then(() => "native cleanup"),
          closing.then(() => "closed without native cleanup"),
        ]),
        "late reader native cleanup",
      ),
      "native cleanup",
      "stream closed without ending its late reader",
    );
    equal(gate.counts().cleanupCalls, 1, "native reader end count");
    equal(gate.counts().cleanupReturns, 1, "native reader end return count");
    equal(
      gate.counts().cleanupCancels,
      0,
      "cleanup reused opening cancellation",
    );
    equal(ended, false, "end did not wait for the cleanup reply");
    equal(reasons.length, 0, "onClose did not wait for the cleanup reply");
    gate.releaseCleanup();
    await within(closing, "stream close");
    equal(
      (await within(read, "cancelled opening read")).done,
      true,
      "read end",
    );
    equal(reasons.length, 1, "onClose count");
    equal(reasons[0].kind, "closed", "onClose reason");
    equal(cleanupAtClose[0], 1, "onClose preceded native cleanup");
    expect(replacement, "onClose did not open a replacement reader");
    const replay = await within(replacement, "same-scope replacement");
    equal(
      (await within(replay.next(), "replacement replay"))?.id,
      id,
      "replay ID",
    );
    await replay.end();
    equal(gate.counts().cleanupCalls, 1, "late reader ended more than once");
  } finally {
    gate.releaseAll();
    try {
      await within(
        Promise.allSettled([
          stream?.end(),
          replacement?.then((reader) => reader.end()),
          client?.end(),
        ]),
        "late reader cleanup",
      );
    } finally {
      gate.session.terminate();
      gate.worker.terminate();
    }
  }
}

// verifies: PROC-031, PROC-041, PROC-042, PROC-052
export async function checkClientEndDuringReaderOpen(
  backendURL: string,
): Promise<void> {
  const gate = heldOpeningConnection();
  const clients: sdk.Client[] = [];
  let stream: sdk.MessageStream | undefined;
  let replay: Awaited<ReturnType<sdk.Group["messageReader"]>> | undefined;
  try {
    const owner = signer();
    const settings = options(
      `ending-reader-${crypto.randomUUID()}.db`,
      backendURL,
    );
    const { client } = await create(gate.session, owner, settings);
    clients.push(client);
    const group = await client.conversations.createGroup([]);
    const id = await group.sendText("owner close must preserve this message");
    const reasons: sdk.StreamCloseReason[] = [];
    gate.arm();
    stream = sdk.MessageStream.openGroup(client, group, undefined, {
      onClose: (reason) => reasons.push(reason),
    });
    await within(gate.opening, "reader opening before client end");
    await within(client.end(), "client end with a held opening reply");
    const { client: reopened } = await within(
      create(gate.session, owner, settings),
      "persistent owner reopen",
    );
    clients.push(reopened);
    equal(reopened.inboxId, client.inboxId, "reopened inbox");
    const restoredGroup = await reopened.conversations.getById(group.id);
    expect(restoredGroup instanceof sdk.Group, "restored group is absent");
    replay = await within(restoredGroup.messageReader(), "reopened reader");
    equal(
      (await within(replay.next(), "owner replay"))?.id,
      id,
      "owner replay ID",
    );
    const closing = stream.end();
    void closing.catch(() => {});
    gate.releaseOpening();
    await within(closing, "old stream close");
    equal(reasons.length, 1, "old stream onClose count");
    equal(reasons[0].kind, "closed", "old stream onClose reason");
    const after = await restoredGroup.sendText("replacement remains open");
    equal(
      (await within(replay.next(), "replacement read"))?.id,
      after,
      "new owner ID",
    );
  } finally {
    gate.releaseAll();
    try {
      await within(
        Promise.allSettled([
          stream?.end(),
          replay?.end(),
          ...clients.map((client) => client.end()),
        ]),
        "reader owner cleanup",
      );
    } finally {
      gate.session.terminate();
      gate.worker.terminate();
    }
  }
}
