import type { StreamCloser } from "@xmtp/node-bindings";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { StreamFailedError } from "@/utils/errors";
import { createStream, type StreamCallback } from "@/utils/streams";

const harness = () => {
  const instances: Array<{
    callback: StreamCallback<number>;
    close: () => void;
    end: ReturnType<typeof vi.fn>;
  }> = [];
  const open = vi.fn(
    async (callback: StreamCallback<number>, close: () => void) => {
      const end = vi.fn();
      instances.push({ callback, close, end });
      return {
        end,
        waitForReady: async () => {},
      } as unknown as StreamCloser;
    },
  );
  return { open, last: () => instances[instances.length - 1]! };
};

describe("notification fallback budget", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("reports a recoverable callback error without ending the wrapper", async () => {
    const native = harness();
    const onError = vi.fn();
    const onEnd = vi.fn();
    const onFail = vi.fn();
    const onRestart = vi.fn();
    const onRetry = vi.fn();
    const stream = await createStream(native.open, undefined, {
      retryAttempts: 1,
      retryDelay: 10,
      onError,
      onEnd,
      onFail,
      onRestart,
      onRetry,
    });
    const transient = new Error("temporary notification failure");
    native.last().callback(transient, undefined);
    expect(onError).toHaveBeenCalledExactlyOnceWith(transient);
    expect(onEnd).not.toHaveBeenCalled();
    expect(stream.isDone).toBe(false);
    await vi.advanceTimersByTimeAsync(10);
    expect(onFail).not.toHaveBeenCalled();
    expect(onRetry).not.toHaveBeenCalled();
    expect(onRestart).not.toHaveBeenCalled();
    expect(native.open).toHaveBeenCalledTimes(1);

    native.last().close();
    expect(onFail).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(10);
    expect(onRestart).toHaveBeenCalledTimes(1);
    native.last().callback(null, 42);
    expect(await stream.next()).toEqual({ done: false, value: 42 });
    expect(onError).toHaveBeenCalledTimes(1);
    expect(onEnd).not.toHaveBeenCalled();

    await stream.end();
    expect(onEnd).toHaveBeenCalledTimes(1);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("ends before reporting exhaustion and rejects one pending read", async () => {
    const native = harness();
    const events: string[] = [];
    const stream = await createStream(native.open, undefined, {
      retryAttempts: 0,
      onEnd: () => events.push("end"),
      onError: (error) => {
        expect(error).toBeInstanceOf(StreamFailedError);
        events.push("error");
      },
    });
    const pending = stream.next();
    native.last().close();
    expect(events).toEqual(["end", "error"]);
    await expect(pending).rejects.toBeInstanceOf(StreamFailedError);
    expect(await stream.next()).toEqual({ done: true, value: undefined });
    expect(vi.getTimerCount()).toBe(0);
  });

  it("keeps one finite budget across quiet successful replacements", async () => {
    const native = harness();
    const onError = vi.fn();
    const stream = await createStream(native.open, undefined, {
      retryAttempts: 2,
      retryDelay: 10,
      onError,
    });
    for (let attempt = 0; attempt < 2; attempt++) {
      native.last().close();
      await vi.advanceTimersByTimeAsync(10);
      native.last().callback(null, attempt);
      expect(await stream.next()).toEqual({ done: false, value: attempt });
      // Core owns healthy network resets. Elapsed JS time does not renew this
      // separate fallback for an unexpected native close.
      await vi.advanceTimersByTimeAsync(60_000);
    }
    native.last().close();
    await vi.advanceTimersByTimeAsync(600_000);
    expect(onError).toHaveBeenCalledExactlyOnceWith(
      expect.any(StreamFailedError),
    );
    expect(native.open).toHaveBeenCalledTimes(3);
    expect(stream.isDone).toBe(true);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("gives a caller-created stream its own full fallback budget", async () => {
    const native = harness();
    const failures: Error[] = [];
    for (let generation = 0; generation < 2; generation++) {
      const stream = await createStream(native.open, undefined, {
        retryAttempts: 1,
        retryDelay: 10,
        onError: (error) => failures.push(error),
      });
      native.last().close();
      await vi.advanceTimersByTimeAsync(10);
      expect(stream.isDone).toBe(false);
      expect(native.open).toHaveBeenCalledTimes((generation + 1) * 2);
      native.last().close();
      expect(stream.isDone).toBe(true);
    }
    expect(failures).toHaveLength(2);
    expect(failures.every((error) => error instanceof StreamFailedError)).toBe(
      true,
    );
    expect(vi.getTimerCount()).toBe(0);
  });
});
