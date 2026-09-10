import type { MessageCatchUp } from "@xmtp/node-bindings";
import { describe, expect, it, vi } from "vitest";
import {
  MessageStream,
  type MessageAcknowledgement,
} from "../src/MessageStream";

const cursor = { databaseId: new Uint8Array(16), deliverySequence: 1n };
const token = () => ({
  checkOwner: vi.fn<MessageAcknowledgement["checkOwner"]>(() => true),
  acknowledge: vi.fn<MessageAcknowledgement["acknowledge"]>(),
  reject: vi.fn<MessageAcknowledgement["reject"]>(),
});
const status: MessageCatchUp = {
  current: {
    scopeGeneration: 1n,
    connectionGeneration: 1n,
    connection: "Connected",
    topics: [],
    discoveryPending: false,
    processing: "Complete",
  },
};
const source = (
  ...messages: Array<{
    message: number;
    cursor: typeof cursor;
    acknowledgement: ReturnType<typeof token>;
  }>
) => ({
  nextDelivery: vi.fn(async () => messages.shift() ?? null),
  close: vi.fn(),
  updateScope: vi.fn(),
  updateFilter: vi.fn(),
  catchUpSnapshot: vi.fn(() => status),
  catchUpChanged: vi.fn(async () => status),
});

describe("MessageStream acknowledgement boundaries", () => {
  it("acknowledges only at the following next request", async () => {
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
    await stream.return();
    expect(second.acknowledge).not.toHaveBeenCalled();
    expect(second.reject).toHaveBeenCalledOnce();
  });

  it("leaves a returned item unacknowledged when iteration ends", async () => {
    const pending = token();
    const reader = source({ message: 1, cursor, acknowledgement: pending });
    const stream = new MessageStream(reader, (value) => value);
    await stream.next();
    await stream.end();
    expect(pending.acknowledge).not.toHaveBeenCalled();
    expect(reader.close).toHaveBeenCalledOnce();
  });

  it("reselects a stale queued item without consuming it", async () => {
    const stale = token();
    stale.checkOwner.mockReturnValue(false);
    const valid = token();
    const reader = source(
      { message: 1, cursor, acknowledgement: stale },
      { message: 2, cursor, acknowledgement: valid },
    );
    const stream = new MessageStream(reader, (value) => value);
    expect((await stream.next()).value).toBe(2);
    expect(stale.acknowledge).not.toHaveBeenCalled();
    expect(reader.close).not.toHaveBeenCalled();
    await stream.end();
  });

  it("reselects when a removed item has no decoded value", async () => {
    const removed = token();
    removed.checkOwner.mockReturnValue(false);
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

  it("rejects the retained token when next-request acknowledgement fails", async () => {
    const pending = token();
    pending.acknowledge.mockRejectedValue(new Error("acknowledgement failed"));
    const reader = source({ message: 1, cursor, acknowledgement: pending });
    const stream = new MessageStream(reader, (value) => value);
    await stream.next();
    await expect(stream.next()).rejects.toThrow("acknowledgement failed");
    expect(pending.reject).toHaveBeenCalledOnce();
    expect(reader.close).toHaveBeenCalledOnce();
    expect(reader.nextDelivery).toHaveBeenCalledOnce();
  });

  it("does not hand off a value after an async decode is cancelled", async () => {
    const pending = token();
    const reader = source({ message: 1, cursor, acknowledgement: pending });
    let finish: ((value: number) => void) | undefined;
    const decoding = new Promise<number>((resolve) => {
      finish = resolve;
    });
    const convert = vi.fn(() => decoding);
    const stream = new MessageStream(reader, convert);
    const next = stream.next();
    await vi.waitFor(() => expect(convert).toHaveBeenCalledOnce());
    await stream.end();
    finish?.(1);
    expect(await next).toEqual({ done: true, value: undefined });
    expect(pending.acknowledge).not.toHaveBeenCalled();
    expect(pending.reject).toHaveBeenCalledOnce();
  });
});

describe("MessageStream callback mode", () => {
  it("starts without iteration and acknowledges each successful callback", async () => {
    const first = token();
    const second = token();
    const reader = source(
      { message: 1, cursor, acknowledgement: first },
      { message: 2, cursor, acknowledgement: second },
    );
    const onEnd = vi.fn();
    const onValue = vi.fn((value: number) => {
      const current = value === 1 ? first : second;
      expect(current.checkOwner).toHaveBeenCalledOnce();
      expect(current.acknowledge).not.toHaveBeenCalled();
    });
    const stream = new MessageStream(reader, (value) => value, {
      onValue,
      onEnd,
    });
    await vi.waitFor(() => expect(onEnd).toHaveBeenCalledOnce());
    expect(onValue.mock.calls).toEqual([[1], [2]]);
    expect(first.acknowledge).toHaveBeenCalledOnce();
    expect(second.acknowledge).toHaveBeenCalledOnce();
    expect(first.reject).not.toHaveBeenCalled();
    expect(second.reject).not.toHaveBeenCalled();
    expect(stream.deliveredCursor).toBe(cursor);
    expect(stream.isDone).toBe(true);
    expect(reader.close).toHaveBeenCalledOnce();
  });

  it("waits for the returned callback promise before acknowledgement or another read", async () => {
    const pending = token();
    const finished = Promise.withResolvers<undefined>();
    const reader = source({ message: 1, cursor, acknowledgement: pending });
    const onValue = vi.fn(() => finished.promise);
    const onEnd = vi.fn();
    const stream = new MessageStream(reader, (value) => value, {
      onValue,
      onEnd,
    });
    await vi.waitFor(() => expect(onValue).toHaveBeenCalledOnce());
    expect(pending.acknowledge).not.toHaveBeenCalled();
    expect(reader.nextDelivery).toHaveBeenCalledOnce();
    await expect(stream.next()).rejects.toThrow("callback mode");
    await expect(stream[Symbol.asyncIterator]().next()).rejects.toThrow(
      "callback mode",
    );
    expect(reader.nextDelivery).toHaveBeenCalledOnce();
    finished.resolve(undefined);
    await vi.waitFor(() => expect(onEnd).toHaveBeenCalledOnce());
    expect(pending.acknowledge).toHaveBeenCalledOnce();
    expect(pending.reject).not.toHaveBeenCalled();
  });

  it.each(["throw", "reject"] as const)(
    "rejects and closes when the callback uses %s",
    async (failure) => {
      const error = new Error("callback failed");
      const pending = token();
      const later = token();
      const reader = source(
        { message: 1, cursor, acknowledgement: pending },
        { message: 2, cursor, acknowledgement: later },
      );
      const onError = vi.fn();
      const onEnd = vi.fn();
      const stream = new MessageStream(reader, (value) => value, {
        onValue: () => {
          if (failure === "throw") throw error;
          return Promise.reject(error);
        },
        onError,
        onEnd,
      });
      await vi.waitFor(() => expect(onEnd).toHaveBeenCalledOnce());
      expect(onError).toHaveBeenCalledExactlyOnceWith(error);
      expect(pending.acknowledge).not.toHaveBeenCalled();
      expect(pending.reject).toHaveBeenCalledOnce();
      expect(reader.close).toHaveBeenCalledOnce();
      expect(reader.nextDelivery).toHaveBeenCalledOnce();
      expect(later.checkOwner).not.toHaveBeenCalled();
      await stream.end();
      expect(onEnd).toHaveBeenCalledOnce();
    },
  );

  it("rejects a callback item immediately on close and never acknowledges it later", async () => {
    const pending = token();
    const finished = Promise.withResolvers<undefined>();
    const reader = source({ message: 1, cursor, acknowledgement: pending });
    const onValue = vi.fn(() => finished.promise);
    const onEnd = vi.fn();
    const stream = new MessageStream(reader, (value) => value, {
      onValue,
      onEnd,
    });
    await vi.waitFor(() => expect(onValue).toHaveBeenCalledOnce());
    await stream.end();
    expect(pending.reject).toHaveBeenCalledOnce();
    expect(onEnd).toHaveBeenCalledOnce();
    finished.resolve(undefined);
    await finished.promise;
    await Promise.resolve();
    expect(pending.acknowledge).not.toHaveBeenCalled();
    expect(pending.reject).toHaveBeenCalledOnce();
    expect(reader.nextDelivery).toHaveBeenCalledOnce();
  });

  it("does not yield between a synchronous ownership check and the callback", async () => {
    const pending = token();
    let current = true;
    pending.checkOwner.mockImplementation(() => {
      queueMicrotask(() => {
        current = false;
      });
      return true;
    });
    const reader = source({ message: 1, cursor, acknowledgement: pending });
    const onValue = vi.fn(() => {
      expect(current).toBe(true);
    });
    const onEnd = vi.fn();
    new MessageStream(reader, (value) => value, { onValue, onEnd });
    await vi.waitFor(() => expect(onEnd).toHaveBeenCalledOnce());
    expect(onValue).toHaveBeenCalledOnce();
    expect(pending.acknowledge).toHaveBeenCalledOnce();
  });

  it("keeps callback mode and its callback stable after construction", async () => {
    const pending = token();
    const reader = source({ message: 1, cursor, acknowledgement: pending });
    const original = vi.fn();
    const replacement = vi.fn();
    const onEnd = vi.fn();
    const options = { onValue: original, onEnd };
    const stream = new MessageStream(reader, (value) => value, options);
    options.onValue = replacement;
    await vi.waitFor(() => expect(onEnd).toHaveBeenCalledOnce());
    expect(original).toHaveBeenCalledOnce();
    expect(replacement).not.toHaveBeenCalled();
    await expect(stream.next()).rejects.toThrow("callback mode");
  });

  it("rejects the pending item when callback acknowledgement fails", async () => {
    const pending = token();
    const error = new Error("acknowledgement failed");
    pending.acknowledge.mockRejectedValue(error);
    const reader = source({ message: 1, cursor, acknowledgement: pending });
    const onValue = vi.fn();
    const onError = vi.fn();
    const onEnd = vi.fn();
    new MessageStream(reader, (value) => value, { onValue, onError, onEnd });
    await vi.waitFor(() => expect(onEnd).toHaveBeenCalledOnce());
    expect(onValue).toHaveBeenCalledOnce();
    expect(onError).toHaveBeenCalledExactlyOnceWith(error);
    expect(pending.reject).toHaveBeenCalledOnce();
    expect(reader.nextDelivery).toHaveBeenCalledOnce();
    expect(reader.close).toHaveBeenCalledOnce();
  });

  it("closes even when callback error handlers throw", async () => {
    const pending = token();
    const reader = source({ message: 1, cursor, acknowledgement: pending });
    const onError = vi.fn(() => {
      throw new Error("error handler failed");
    });
    const onEnd = vi.fn(() => {
      throw new Error("end handler failed");
    });
    new MessageStream(reader, (value) => value, {
      onValue: () => {
        throw new Error("callback failed");
      },
      onError,
      onEnd,
    });
    await vi.waitFor(() => expect(onEnd).toHaveBeenCalledOnce());
    expect(onError).toHaveBeenCalledOnce();
    expect(reader.close).toHaveBeenCalledOnce();
    expect(pending.reject).toHaveBeenCalledOnce();
    expect(pending.acknowledge).not.toHaveBeenCalled();
  });
});
