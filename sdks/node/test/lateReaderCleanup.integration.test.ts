import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { setImmediate as nextTurn } from "node:timers/promises";

import { createRegisteredClient, createSigner } from "@test/helpers";
import { Group, type Client } from "@xmtp/node-sdk";
import { expect, it } from "vitest";

// This private adapter fixture uses real readers from the staged native SDK.
// It holds their result before the shared host stream can adopt it.
import { unwrapGroup } from "../dist/public-values.gen.js";
import {
  ReaderStream,
  type ReaderLike,
  type StreamCloseReason,
} from "../dist/runtime/streams/reader.js";
import { XmtpError as NativeXmtpError } from "../dist/xmtp_sdk.js";

function gate() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

async function within<T>(operation: Promise<T>, label: string): Promise<T> {
  let timer: ReturnType<typeof setTimeout>;
  try {
    return await Promise.race([
      operation,
      new Promise<never>((_, reject) => {
        timer = setTimeout(() => reject(new Error(label)), 20_000);
      }),
    ]);
  } finally {
    clearTimeout(timer!);
  }
}

// verifies: PROC-041, PROC-042
it.each(["end", "abort", "client-end"] as const)(
  "cleans up a real late reader after %s and replays its unacknowledged item",
  async (mode) => {
    const directory = await mkdtemp(join(tmpdir(), "xmtp-node-late-reader-"));
    const storage = {
      location: {
        dbPath: join(directory, "client.db3"),
        attachmentsDir: join(directory, "attachments"),
      },
    };
    const signer = createSigner().signer;
    const clients: Client[] = [];
    // Keep every native wrapper alive through the assertions. Cleanup must
    // release ownership without GC or a private native destroy call.
    const retained: ReaderLike<unknown>[] = [];
    const acquired = gate();
    const release = gate();
    const abort = new AbortController();
    const closeReasons: StreamCloseReason[] = [];
    let stream: ReaderStream<unknown> | undefined;
    let consumption: Promise<void> | undefined;
    let close: Promise<void> | undefined;
    let lateEndCalls = 0;
    let lateEndCompleted = false;
    let lateEndError: unknown;
    let callbacks = 0;
    try {
      const client = await createRegisteredClient(signer, { storage });
      clients.push(client);
      const group = await client.conversations.createGroup([]);
      const nativeGroup = unwrapGroup(group);
      const messageId = await nativeGroup.sendText(
        "late native reader",
        undefined,
      );
      const history = await nativeGroup.messageHistorySnapshot(128);
      const firstPosition = history.messages.findIndex(
        (message) => message.id.toString() === messageId.toString(),
      );
      expect(firstPosition).toBeGreaterThanOrEqual(0);
      type NativeReader = Awaited<ReturnType<typeof nativeGroup.messageReader>>;
      let replacement: Promise<NativeReader> | undefined;
      const openNative = async (signal?: AbortSignal) => {
        const reader = await nativeGroup.messageReader(
          undefined,
          signal ? { signal } : undefined,
        );
        retained.push(reader);
        return reader;
      };
      stream = new ReaderStream(
        async (signal) => {
          const reader = await openNative(signal);
          // Drain the exact creation-history prefix before the target item.
          let item = await reader.next({ signal });
          for (let position = 0; position <= firstPosition; position++) {
            if (position > 0) item = await reader.next({ signal });
            expect(item?.id.toString()).toBe(
              history.messages[position]?.id.toString(),
            );
            expect(item?.deliveryCursor).toEqual(
              history.messages[position]?.deliveryCursor,
            );
          }
          // A real native handoff holds the scope and leaves this item
          // unacknowledged. The host has not received the reader or value.
          expect(item?.id.toString()).toBe(messageId.toString());
          acquired.resolve();
          await release.promise;
          return {
            next: (options) => reader.next(options),
            async end() {
              lateEndCalls++;
              try {
                await reader.end();
                lateEndCompleted = true;
              } catch (error) {
                lateEndError = error;
                throw error;
              }
            },
          };
        },
        client,
        {
          signal: abort.signal,
          onClose(reason) {
            closeReasons.push(reason);
            if (mode !== "client-end") {
              // Scope acquisition starts in onClose. It must not depend on
              // a later GC pass or on ending the owning client.
              replacement = openNative();
              void replacement.catch(() => {});
            }
          },
        },
      );
      consumption = stream.onValue(() => {
        callbacks++;
      });
      void consumption.catch(() => {});
      await within(acquired.promise, "native reader was not acquired");
      await assert.rejects(
        within(openNative(), "owned scope check hung"),
        NativeXmtpError.ConsumerOwned,
      );

      if (mode === "client-end") await within(client.end(), "client end hung");
      if (mode === "abort") abort.abort();
      close = stream.end();
      let closeSettled = false;
      void close.then(() => {
        closeSettled = true;
      });
      await nextTurn();
      expect(closeSettled).toBe(false);
      expect(closeReasons).toEqual([]);
      release.resolve();
      await within(close, "late reader cleanup hung");
      await within(consumption, "closed stream kept consuming");

      if (mode === "client-end") {
        // The old wrapper stays alive. Core client shutdown must release
        // ownership even if late reader.end is refused by the closed client.
        const reopened = await createRegisteredClient(signer, { storage });
        clients.push(reopened);
        const reopenedGroup = await reopened.conversations.getById(group.id);
        assert.ok(reopenedGroup instanceof Group);
        const replay = await unwrapGroup(reopenedGroup).messageReader();
        retained.push(replay);
        expect(
          (await within(replay.next(), "reopened replay hung"))?.id.toString(),
        ).toBe(messageId.toString());
        expect(lateEndError).toBeInstanceOf(NativeXmtpError.ClientClosed);
      } else {
        assert.ok(replacement, "onClose did not attempt scope acquisition");
        const replay = await within(
          replacement,
          "late reader still owns the scope",
        );
        expect(
          (
            await within(replay.next(), "same-client replay hung")
          )?.id.toString(),
        ).toBe(messageId.toString());
        expect(lateEndCompleted).toBe(true);
        expect(lateEndError).toBeUndefined();
      }
      expect(lateEndCalls).toBe(1);
      expect(callbacks).toBe(0);
      expect(closeReasons.map((reason) => reason.kind)).toEqual(["closed"]);
      expect((await stream.next()).done).toBe(true);
      expect(retained.length).toBeGreaterThanOrEqual(2);
    } finally {
      release.resolve();
      if (stream)
        await within(stream.end(), "stream cleanup hung").catch(() => {});
      if (consumption)
        await within(consumption, "consumer cleanup hung").catch(() => {});
      await Promise.allSettled(
        retained.map((reader) => within(reader.end(), "reader cleanup hung")),
      );
      const shutdown = await Promise.allSettled(
        clients.map((client) => within(client.end(), "client cleanup hung")),
      );
      for (const result of shutdown) {
        if (result.status === "rejected") throw result.reason;
      }
      await rm(directory, { recursive: true, force: true });
    }
  },
);
