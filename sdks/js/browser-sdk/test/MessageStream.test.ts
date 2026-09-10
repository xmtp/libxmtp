import type { MessageCatchUp } from "@xmtp/wasm-bindings";
import { describe, expect, it, vi } from "vitest";
import { MessageStream } from "../src/MessageStream";

const cursor = { databaseId: new Uint8Array(16), deliverySequence: 1n };
const token = () => ({
  checkOwner: vi.fn(async () => true),
  acknowledge: vi.fn(async () => {}),
  reject: vi.fn(async () => {}),
});
const status: MessageCatchUp = {
  current: {
    scopeGeneration: 1n,
    connectionGeneration: 1n,
    connection: "Connected",
    topics: [],
    discoveryPending: false,
    processing: "Complete",
    errorCode: undefined,
  },
  previous: undefined,
};
const source = (
  ...messages: Array<{
    message: number;
    cursor: typeof cursor;
    acknowledgement: ReturnType<typeof token>;
  }>
) => ({
  nextDelivery: vi.fn(async () => messages.shift()),
  close: vi.fn(),
  updateScope: vi.fn(),
  updateFilter: vi.fn(),
  catchUpSnapshot: vi.fn(() => status),
  catchUpChanged: vi.fn(async () => status),
});
const deferred = <T>() => {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((complete) => {
    resolve = complete;
  });
  return { promise, resolve };
};

describe("MessageStream worker acknowledgement boundaries", () => {
  it("acknowledges at the following next request, not worker receipt", async () => {
    const first = token();
    const second = token();
    const reader = source(
      { message: 1, cursor, acknowledgement: first },
      { message: 2, cursor, acknowledgement: second },
    );
    const stream = new MessageStream(reader, (value) => value);
    expect((await stream.next()).value).toBe(1);
    expect(first.acknowledge).not.toHaveBeenCalled();
    expect((await stream.next()).value).toBe(2);
    expect(first.acknowledge).toHaveBeenCalledOnce();
    await stream.end();
    expect(second.acknowledge).not.toHaveBeenCalled();
    expect(second.reject).toHaveBeenCalledOnce();
  });

  it("discards a stale worker token and selects again", async () => {
    const stale = token();
    stale.checkOwner.mockResolvedValue(false);
    const valid = token();
    const reader = source(
      { message: 1, cursor, acknowledgement: stale },
      { message: 2, cursor, acknowledgement: valid },
    );
    const stream = new MessageStream(reader, (value) => value);
    expect((await stream.next()).value).toBe(2);
    expect(stale.acknowledge).not.toHaveBeenCalled();
    await stream.end();
  });

  it("reselects when a removed worker item has no decoded value", async () => {
    const removed = token();
    removed.checkOwner.mockResolvedValue(false);
    const retained = token();
    const reader = source(
      { message: 1, cursor, acknowledgement: removed },
      { message: 2, cursor, acknowledgement: retained },
    );
    const stream = new MessageStream(reader, (value) =>
      value === 1 ? undefined : value,
    );
    expect(await stream.next()).toEqual({ done: false, value: 2 });
    expect(removed.checkOwner).toHaveBeenCalledOnce();
    expect(removed.acknowledge).not.toHaveBeenCalled();
    expect(removed.reject).toHaveBeenCalledOnce();
    expect(reader.close).not.toHaveBeenCalled();
    await stream.end();
  });

  it.each(["throw", "reject"] as const)(
    "closes once when a callback uses %s without an iterator or unhandled rejection",
    async (failure) => {
      const pending = token();
      const error = new Error("callback failed");
      const reader = source({ message: 1, cursor, acknowledgement: pending });
      const onValue = vi.fn(() => {
        if (failure === "throw") throw error;
        return Promise.reject(error);
      });
      const onError = vi.fn();
      const onEnd = vi.fn();
      const stream = new MessageStream(reader, (value) => value, {
        onValue,
        onError,
        onEnd,
      });
      await vi.waitFor(() => expect(onEnd).toHaveBeenCalledOnce());
      await stream.end();
      expect(onValue).toHaveBeenCalledOnce();
      expect(onError).toHaveBeenCalledExactlyOnceWith(error);
      expect(onEnd).toHaveBeenCalledOnce();
      expect(pending.acknowledge).not.toHaveBeenCalled();
      expect(pending.reject).toHaveBeenCalledOnce();
      expect(reader.close).toHaveBeenCalledOnce();
      expect(reader.nextDelivery).toHaveBeenCalledOnce();
      expect(stream.isDone).toBe(true);
    },
  );

  it("does not call the app when an ownership check finishes after close", async () => {
    const pending = token();
    let finish: ((valid: boolean) => void) | undefined;
    pending.checkOwner.mockImplementation(
      () =>
        new Promise<boolean>((resolve) => {
          finish = resolve;
        }),
    );
    const reader = source({ message: 1, cursor, acknowledgement: pending });
    const onValue = vi.fn();
    const stream = new MessageStream(reader, (value) => value, { onValue });
    await vi.waitFor(() => expect(pending.checkOwner).toHaveBeenCalledOnce());
    await stream.end();
    finish?.(true);
    await vi.waitFor(() => expect(pending.reject).toHaveBeenCalledOnce());
    expect(onValue).not.toHaveBeenCalled();
    expect(pending.acknowledge).not.toHaveBeenCalled();
    expect(pending.reject).toHaveBeenCalledOnce();
  });

  it("automatically checks, calls, and acknowledges synchronous callbacks in order", async () => {
    const events: string[] = [];
    const first = token();
    const second = token();
    for (const [index, acknowledgement] of [first, second].entries()) {
      acknowledgement.checkOwner.mockImplementation(async () => {
        events.push(`check ${index + 1}`);
        return true;
      });
      acknowledgement.acknowledge.mockImplementation(async () => {
        events.push(`acknowledge ${index + 1}`);
      });
    }
    const reader = source(
      { message: 1, cursor, acknowledgement: first },
      { message: 2, cursor, acknowledgement: second },
    );
    const onValue = vi.fn((value: number) => {
      expect(
        value === 1 ? first.acknowledge : second.acknowledge,
      ).not.toHaveBeenCalled();
      events.push(`callback ${value}`);
    });
    const onEnd = vi.fn();
    const stream = new MessageStream(
      reader,
      (value) => {
        events.push(`decode ${value}`);
        return value;
      },
      { onValue, onEnd },
    );
    await vi.waitFor(() => expect(onEnd).toHaveBeenCalledOnce());
    expect(events).toEqual([
      "decode 1",
      "check 1",
      "callback 1",
      "acknowledge 1",
      "decode 2",
      "check 2",
      "callback 2",
      "acknowledge 2",
    ]);
    expect(onValue.mock.calls).toEqual([[1], [2]]);
    expect(first.reject).not.toHaveBeenCalled();
    expect(second.reject).not.toHaveBeenCalled();
    expect(reader.close).toHaveBeenCalledOnce();
    expect(stream.isDone).toBe(true);
  });

  it("waits for an async callback before acknowledgement or the next read", async () => {
    const first = token();
    const second = token();
    const release = deferred<undefined>();
    const reader = source(
      { message: 1, cursor, acknowledgement: first },
      { message: 2, cursor, acknowledgement: second },
    );
    const onValue = vi.fn(async (value: number) => {
      if (value === 1) await release.promise;
    });
    const onEnd = vi.fn();
    const stream = new MessageStream(reader, (value) => value, {
      onValue,
      onEnd,
    });
    try {
      await vi.waitFor(() => expect(onValue).toHaveBeenCalledOnce());
      expect(reader.nextDelivery).toHaveBeenCalledOnce();
      expect(first.acknowledge).not.toHaveBeenCalled();
      expect(second.checkOwner).not.toHaveBeenCalled();
      release.resolve(undefined);
      await vi.waitFor(() => expect(onEnd).toHaveBeenCalledOnce());
      expect(onValue.mock.calls).toEqual([[1], [2]]);
      expect(first.acknowledge).toHaveBeenCalledOnce();
      expect(second.acknowledge).toHaveBeenCalledOnce();
      expect(first.reject).not.toHaveBeenCalled();
      expect(second.reject).not.toHaveBeenCalled();
    } finally {
      release.resolve(undefined);
      await stream.end();
    }
  });

  it("rejects a pending callback on close and does not acknowledge its later return", async () => {
    const pending = token();
    const release = deferred<undefined>();
    const finished = deferred<undefined>();
    const reader = source({ message: 1, cursor, acknowledgement: pending });
    const onValue = vi.fn(async () => {
      await release.promise;
      finished.resolve(undefined);
    });
    const onEnd = vi.fn();
    const stream = new MessageStream(reader, (value) => value, {
      onValue,
      onEnd,
    });
    try {
      await vi.waitFor(() => expect(onValue).toHaveBeenCalledOnce());
      await stream.end();
      expect(pending.reject).toHaveBeenCalledOnce();
      expect(pending.acknowledge).not.toHaveBeenCalled();
      expect(reader.close).toHaveBeenCalledOnce();
      expect(onEnd).toHaveBeenCalledOnce();
      release.resolve(undefined);
      await finished.promise;
      await new Promise((resolve) => setTimeout(resolve, 0));
      expect(pending.acknowledge).not.toHaveBeenCalled();
      expect(pending.reject).toHaveBeenCalledOnce();
      expect(reader.nextDelivery).toHaveBeenCalledOnce();
      expect(onEnd).toHaveBeenCalledOnce();
    } finally {
      release.resolve(undefined);
      await stream.end();
    }
  });

  it("rejects next and for-await in callback mode without a second consumer", async () => {
    const pending = token();
    const release = deferred<undefined>();
    const reader = source({ message: 1, cursor, acknowledgement: pending });
    const onValue = vi.fn(() => release.promise);
    const stream = new MessageStream(reader, (value) => value, { onValue });
    try {
      await vi.waitFor(() => expect(onValue).toHaveBeenCalledOnce());
      await expect(stream.next()).rejects.toThrow(/callback mode/i);
      const iterated: Array<number | undefined> = [];
      const consume = async () => {
        for await (const value of stream) iterated.push(value);
      };
      await expect(consume()).rejects.toThrow(/callback mode/i);
      expect(iterated).toEqual([]);
      expect(reader.nextDelivery).toHaveBeenCalledOnce();
      expect(onValue).toHaveBeenCalledOnce();
      expect(pending.acknowledge).not.toHaveBeenCalled();
      expect(reader.close).not.toHaveBeenCalled();
    } finally {
      await stream.end();
      release.resolve(undefined);
    }
  });

  it("keeps the callback selected at construction when options change", async () => {
    const pending = token();
    const reader = source({ message: 1, cursor, acknowledgement: pending });
    const original = vi.fn();
    const replacement = vi.fn();
    const onEnd = vi.fn();
    const options = { onValue: original, onEnd };
    const stream = new MessageStream(reader, (value) => value, options);
    options.onValue = replacement;
    await vi.waitFor(() => expect(onEnd).toHaveBeenCalledOnce());
    expect(original).toHaveBeenCalledExactlyOnceWith(1);
    expect(replacement).not.toHaveBeenCalled();
    expect(pending.acknowledge).toHaveBeenCalledOnce();
    await expect(stream.next()).rejects.toThrow(/callback mode/i);
  });

  it("keeps iterator mode when a callback is added after construction", async () => {
    const pending = token();
    const reader = source({ message: 1, cursor, acknowledgement: pending });
    const options: { onValue?: (value: number) => void } = {};
    const stream = new MessageStream(reader, (value) => value, options);
    const added = vi.fn();
    options.onValue = added;
    expect(reader.nextDelivery).not.toHaveBeenCalled();
    expect(await stream.next()).toEqual({ done: false, value: 1 });
    expect(added).not.toHaveBeenCalled();
    expect(pending.acknowledge).not.toHaveBeenCalled();
    await stream.end();
    expect(pending.reject).toHaveBeenCalledOnce();
  });
});
