// The hand-written host stream, event stream, listener gate and Timestamp in
// apps/xmtp_sdk_bindgen/runtime/ts, driven with fake readers and a fake
// binding. Rust tests cover the readers and listeners themselves
// (crates/xmtp_sdk/src/tests/reader_*.rs, event_*.rs).
import { describe, expect, it, vi } from "vitest";

import { Client } from "../../../apps/xmtp_sdk_bindgen/runtime/ts/client";
import { EventStream } from "../../../apps/xmtp_sdk_bindgen/runtime/ts/events/reader";
import { Timestamp } from "../../../apps/xmtp_sdk_bindgen/runtime/ts/ids";
import {
  ConversationStream,
  MessageStream,
  type ReaderLike,
  type StreamCloseReason,
} from "../../../apps/xmtp_sdk_bindgen/runtime/ts/streams/reader";
import { ConnectionState } from "../../../apps/xmtp_sdk_bindgen/runtime/xmtp_sdk";

const owner = {};
const idle = (): ReaderLike<never> => ({
  next: async () => undefined,
  end: async () => {},
});
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

/** A reader scope that one open reader at a time may hold. */
function scope() {
  let owned = false;
  return async (): Promise<ReaderLike<number>> => {
    if (owned)
      throw Object.assign(new Error("owned"), { code: "ConsumerOwned" });
    owned = true;
    return {
      next: async () => undefined,
      end: async () => {
        await tick();
        owned = false;
      },
    };
  };
}

async function noUnhandledRejection(action: () => Promise<void>) {
  const unhandled: unknown[] = [];
  const capture = (error: unknown) => unhandled.push(error);
  process.on("unhandledRejection", capture);
  try {
    await action();
    await tick();
    expect(unhandled).toEqual([]);
  } finally {
    process.off("unhandledRejection", capture);
  }
}

describe("host reader stream", () => {
  // verifies: PROC-041, PROC-042
  it.each(["end", "fail"] as const)(
    "closes once after %s and only after its reader released the scope",
    async (mode) => {
      for (const Stream of [MessageStream, ConversationStream]) {
        const reasons: StreamCloseReason[] = [];
        const stream = new Stream(async () => idle(), owner, {
          onClose: (reason) => reasons.push(reason),
        });
        await stream.end();
        await stream.end();
        expect(reasons.map((reason) => reason.kind)).toEqual(["closed"]);
      }
      const open = scope();
      const failure = new Error("read failed");
      let replacement: MessageStream<number> | undefined;
      const stream = new MessageStream(
        async () => ({
          ...(await open()),
          next: async () => {
            if (mode === "fail") throw failure;
            return undefined;
          },
        }),
        owner,
        { onClose: () => (replacement = new MessageStream(open, owner)) },
      );
      await stream.ready();
      if (mode === "end") await stream.end();
      else await expect(stream.next()).rejects.toBe(failure);
      await replacement!.ready();
      await replacement!.end();
    },
  );

  it.each(["end", "fail"] as const)(
    "a second close waits for the reader teardown a first %s started",
    async (first) => {
      let ends = 0;
      let release!: () => void;
      const endStarted = Promise.withResolvers<void>();
      const failure = new Error("read failed");
      const stream = new MessageStream(
        async () => ({
          next: async () => {
            throw failure;
          },
          end: async () => {
            ends++;
            endStarted.resolve();
            await new Promise<void>((resolve) => (release = resolve));
          },
        }),
        owner,
      );
      await stream.ready();
      const closing =
        first === "end"
          ? stream.end()
          : expect(stream.next()).rejects.toBe(failure);
      await endStarted.promise;
      let settled = false;
      const second = stream.end().then(() => (settled = true));
      await tick();
      expect(settled).toBe(false);
      release();
      await Promise.all([closing, second]);
      expect(ends).toBe(1);
    },
  );

  it("a throwing onClose still ends the reader on end, read failure and open failure", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    const onClose = () => {
      throw new Error("close callback failed");
    };
    await noUnhandledRejection(async () => {
      for (const failRead of [false, true]) {
        let ended = false;
        const stream = new MessageStream(
          async () => ({
            next: async () => {
              if (failRead) throw new Error("read failed");
              return undefined;
            },
            end: async () => void (ended = true),
          }),
          owner,
          { onClose },
        );
        await stream.ready();
        if (failRead)
          await expect(stream.next()).rejects.toThrow("read failed");
        else await stream.end();
        expect(ended).toBe(true);
      }
      const failure = new Error("open failed");
      const closes: StreamCloseReason[] = [];
      const failedOpen = new MessageStream(
        async () => {
          throw failure;
        },
        owner,
        { onClose: (reason) => (closes.push(reason), onClose()) },
      );
      await expect(failedOpen.next()).rejects.toBe(failure);
      expect(closes).toEqual([{ kind: "failed", error: failure }]);
    });
    vi.restoreAllMocks();
  });

  it.each([true, false])(
    "an abort before opening (%s) or after it leaves no reader open",
    async (beforeOpen) => {
      const controller = new AbortController();
      if (beforeOpen) controller.abort();
      let opened = false;
      let ended = false;
      const stream = new MessageStream(
        async () => {
          opened = true;
          return {
            next: async () => undefined,
            end: async () => void (ended = true),
          };
        },
        owner,
        { signal: controller.signal },
      );
      if (!beforeOpen) {
        await stream.ready();
        controller.abort();
      }
      await tick();
      expect(opened).toBe(!beforeOpen);
      expect(ended).toBe(!beforeOpen);
      expect((await stream.next()).done).toBe(true);
    },
  );

  // verifies: PROC-044
  it("reports the state at subscription, then each change in order", async () => {
    for (const Stream of [MessageStream, ConversationStream]) {
      const states: [ConnectionState | undefined, ConnectionState][] = [];
      const changes: ((state: ConnectionState) => void)[] = [];
      const stream = new Stream(
        async () => ({
          ...idle(),
          connectionState: async () => ConnectionState.Connected,
          connectionStateChanged: () =>
            new Promise<ConnectionState>((resolve) => changes.push(resolve)),
        }),
        owner,
        {
          onConnectionStateChange: (previous, current) =>
            states.push([previous, current]),
        },
      );
      await stream.ready();
      for (const next of [
        ConnectionState.Reconnecting,
        ConnectionState.Connected,
      ]) {
        await vi.waitFor(() => expect(changes).toHaveLength(1));
        changes.shift()!(next);
      }
      await vi.waitFor(() => expect(states).toHaveLength(3));
      expect(states).toEqual([
        [undefined, ConnectionState.Connected],
        [ConnectionState.Connected, ConnectionState.Reconnecting],
        [ConnectionState.Reconnecting, ConnectionState.Connected],
      ]);
      await stream.end();
    }
    // A closed state ends the monitor, and a throwing app callback stays
    // contained in it.
    let polls = 0;
    let calls = 0;
    await noUnhandledRejection(async () => {
      for (const state of [ConnectionState.Closed, ConnectionState.Connected]) {
        const stream = new MessageStream(
          async () => ({
            ...idle(),
            connectionState: async () => state,
            connectionStateChanged: async () => {
              polls++;
              await tick();
              return ConnectionState.Closed;
            },
          }),
          owner,
          {
            onConnectionStateChange: () => {
              calls++;
              if (state === ConnectionState.Connected) throw new Error("app");
            },
          },
        );
        await stream.ready();
        await tick();
        await stream.end();
      }
    });
    expect([polls, calls]).toEqual([0, 2]);
  });

  // verifies: PROC-041
  it("a failed onValue callback closes the stream as failed and reads no further item", async () => {
    let reads = 0;
    const reasons: StreamCloseReason[] = [];
    const stream = new MessageStream(
      async () => ({ next: async () => ++reads, end: async () => {} }),
      owner,
      { onClose: (reason) => reasons.push(reason) },
    );
    const failure = new Error("callback failed");
    await expect(
      stream.onValue((value) => {
        if (value === 2) throw failure;
      }),
    ).rejects.toBe(failure);
    // The next read would acknowledge item 2, so it never starts.
    expect(reads).toBe(2);
    expect(reasons).toEqual([{ kind: "failed", error: failure }]);
  });

  it("an iterator read does not prefetch the next item", async () => {
    let reads = 0;
    const stream = new MessageStream(
      async () => ({ next: async () => ++reads, end: async () => {} }),
      owner,
    );
    expect((await stream.next()).value).toBe(1);
    await tick();
    expect(reads).toBe(1);
    await stream.end();
  });

  it("return settles a pending read, and a reader end failure keeps the read error", async () => {
    const pending = new MessageStream(
      async () => ({
        next: () => new Promise<never>(() => {}),
        end: async () => {},
      }),
      owner,
    );
    const read = pending.next();
    await pending.ready();
    await pending.return();
    expect(await read).toEqual({ done: true, value: undefined });
    const failure = new Error("read failed");
    const failing = new MessageStream(
      async () => ({
        next: async () => {
          throw failure;
        },
        end: async () => {
          throw new Error("end failed");
        },
      }),
      owner,
    );
    await expect(failing.next()).rejects.toBe(failure);
  });

  it("an opener that settles after end leaves the stream closed, not failed", async () => {
    const opener = Promise.withResolvers<ReaderLike<never>>();
    const reasons: StreamCloseReason[] = [];
    const stream = new MessageStream(() => opener.promise, owner, {
      onClose: (reason) => reasons.push(reason),
    });
    await tick();
    const ending = stream.end();
    opener.reject(new Error("open failed after end"));
    await ending;
    expect(reasons).toEqual([{ kind: "closed" }]);
    expect((await stream.next()).done).toBe(true);
  });
});

describe("host event stream and listeners", () => {
  it("return ends the reader once and settles a pending read", async () => {
    let ends = 0;
    const events = new EventStream({
      next: ({ signal } = { signal: new AbortController().signal }) =>
        new Promise((resolve) =>
          signal.addEventListener("abort", () => resolve(undefined)),
        ),
      end: async () => void ends++,
    } as never);
    const pending = events.next();
    await events.return();
    await events.return();
    expect(await pending).toEqual({ done: true, value: undefined });
    expect(ends).toBe(1);
  });

  // verifies: EVENT-053
  it("a listener stopped or ended before its callback starts never calls the app", async () => {
    const dispatch: { onEvent(event: unknown): Promise<void> }[] = [];
    const startHeld = Promise.withResolvers<void>();
    const raw = {
      clientKey: () => 1n,
      startListener: async (_: unknown, listener: (typeof dispatch)[0]) => {
        dispatch.push(listener);
        if (dispatch.length === 2) await startHeld.promise;
        return BigInt(dispatch.length);
      },
      stopListener: async () => {},
      end: async () => {},
    };
    const client = new (Client as unknown as new (
      raw: unknown,
      codecs: unknown[],
    ) => Client)(raw, []);
    const callback = vi.fn();
    const filter = { kinds: [], references_own_messages: false } as never;
    await client.stopListener(await client.startListener(filter, callback));
    await dispatch[0]!.onEvent({});
    const starting = client.startListener(filter, callback);
    await vi.waitFor(() => expect(dispatch).toHaveLength(2));
    await client.end();
    startHeld.resolve();
    await starting;
    await dispatch[1]!.onEvent({});
    expect(callback).not.toHaveBeenCalled();
  });
});

it("Timestamp dates round nanoseconds down to the millisecond", () => {
  expect(new Timestamp(-1n).date.getTime()).toBe(-1);
  expect(new Timestamp(-1_000_000n).date.getTime()).toBe(-1);
  expect(new Timestamp(-1_000_001n).date.getTime()).toBe(-2);
  expect(new Timestamp(999_999n).date.getTime()).toBe(0);
  expect(new Timestamp(1_000_000n).date.getTime()).toBe(1);
});
