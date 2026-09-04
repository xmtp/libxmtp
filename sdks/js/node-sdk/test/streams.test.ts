import type { StreamCloser } from "@xmtp/node-bindings";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { StreamFailedError } from "@/utils/errors";
import { createStream, type StreamCallback } from "@/utils/streams";

const makeCloser = () => ({
  end: vi.fn(),
  endAndWait: vi.fn().mockResolvedValue(undefined),
  isClosed: vi.fn().mockReturnValue(false),
  waitForReady: vi.fn().mockResolvedValue(undefined),
});

type MockCloser = ReturnType<typeof makeCloser>;

type StreamInstance = {
  callback: StreamCallback<number>;
  onFail: () => void;
  closer: MockCloser;
};

// Captures every native stream created by createStream so tests can drive
// the value callback and the native close (onFail) callback directly.
const makeHarness = () => {
  const instances: StreamInstance[] = [];
  const streamFunction = vi.fn(
    async (callback: StreamCallback<number>, onFail: () => void) => {
      const closer = makeCloser();
      instances.push({ callback, onFail, closer });
      return closer as unknown as StreamCloser;
    },
  );
  const last = () => instances[instances.length - 1];
  return { instances, streamFunction, last };
};

describe("createStream lifecycle", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("ends the native stream when the consumer ends the stream", async () => {
    const { instances, streamFunction } = makeHarness();
    const stream = await createStream<number>(streamFunction, undefined, {
      retryDelay: 1000,
    });

    await stream.end();

    expect(instances[0].closer.end).toHaveBeenCalled();
  });

  it("does not restart after end() during a pending retry", async () => {
    const { streamFunction, last } = makeHarness();
    const stream = await createStream<number>(streamFunction, undefined, {
      retryDelay: 1000,
    });

    // native close schedules a retry
    last().onFail();
    // consumer ends the stream before the retry delay expires
    await stream.end();
    await vi.advanceTimersByTimeAsync(10_000);

    expect(streamFunction).toHaveBeenCalledTimes(1);
    // the pending retry timer must be cancelled
    expect(vi.getTimerCount()).toBe(0);
  });

  it("immediately closes a native stream created after end()", async () => {
    const instances: StreamInstance[] = [];
    let resolveSecond!: (closer: StreamCloser) => void;
    const secondCloser = makeCloser();
    const streamFunction = vi.fn(
      (callback: StreamCallback<number>, onFail: () => void) => {
        if (instances.length === 0) {
          const closer = makeCloser();
          instances.push({ callback, onFail, closer });
          return Promise.resolve(closer as unknown as StreamCloser);
        }
        // second call: retry creation stays in flight until the test resolves it
        instances.push({
          callback,
          onFail,
          closer: secondCloser,
        });
        return new Promise<StreamCloser>((resolve) => {
          resolveSecond = resolve;
        });
      },
    );
    const onRestart = vi.fn();
    const stream = await createStream<number>(streamFunction, undefined, {
      onRestart,
      retryDelay: 1000,
    });

    // native close schedules a retry, then the retry enters streamFunction
    instances[0].onFail();
    await vi.advanceTimersByTimeAsync(1000);
    expect(streamFunction).toHaveBeenCalledTimes(2);

    // consumer ends the stream while the retry creation is in flight
    await stream.end();
    resolveSecond(secondCloser as unknown as StreamCloser);
    await vi.advanceTimersByTimeAsync(0);

    expect(secondCloser.end).toHaveBeenCalled();
    expect(onRestart).not.toHaveBeenCalled();
  });

  it("suppresses onValue and onError after end()", async () => {
    const { instances, streamFunction } = makeHarness();
    const onValue = vi.fn();
    const onError = vi.fn();
    const stream = await createStream<number>(streamFunction, undefined, {
      onError,
      onValue,
    });

    await stream.end();
    instances[0].callback(null, 1);
    instances[0].callback(new Error("boom"), undefined);

    expect(onValue).not.toHaveBeenCalled();
    expect(onError).not.toHaveBeenCalled();
  });

  it("does not emit a value whose async mutation resolves after end()", async () => {
    const { instances, streamFunction } = makeHarness();
    let resolveMutation!: (value: number) => void;
    const mutator = vi.fn(
      () =>
        new Promise<number>((resolve) => {
          resolveMutation = resolve;
        }),
    );
    const onValue = vi.fn();
    const stream = await createStream<number, number>(streamFunction, mutator, {
      onValue,
    });

    // a value arrives and its async mutation is in flight
    instances[0].callback(null, 1);
    await stream.end();
    resolveMutation(2);
    await vi.advanceTimersByTimeAsync(0);

    expect(onValue).not.toHaveBeenCalled();
  });

  it("allows only one retry in flight per stream", async () => {
    const { streamFunction, last } = makeHarness();
    await createStream<number>(streamFunction, undefined, {
      retryDelay: 1000,
    });

    // multiple native close callbacks before the retry timer expires
    last().onFail();
    last().onFail();
    last().onFail();
    await vi.advanceTimersByTimeAsync(5000);

    // initial stream + exactly one replacement
    expect(streamFunction).toHaveBeenCalledTimes(2);
  });

  it("stays silent when end() precedes the native close callback", async () => {
    const { instances, streamFunction } = makeHarness();
    const onError = vi.fn();
    const onFail = vi.fn();
    const stream = await createStream<number>(streamFunction, undefined, {
      onError,
      onFail,
      retryDelay: 1000,
    });

    await stream.end();
    // ending the native stream triggers its close callback
    instances[0].onFail();
    await vi.advanceTimersByTimeAsync(10_000);

    expect(onFail).not.toHaveBeenCalled();
    expect(onError).not.toHaveBeenCalled();
    expect(streamFunction).toHaveBeenCalledTimes(1);
  });

  it("stays silent when end() precedes the native close callback with retryOnFail disabled", async () => {
    const { instances, streamFunction } = makeHarness();
    const onError = vi.fn();
    const onFail = vi.fn();
    const stream = await createStream<number>(streamFunction, undefined, {
      onError,
      onFail,
      retryOnFail: false,
    });

    await stream.end();
    instances[0].onFail();
    await vi.advanceTimersByTimeAsync(10_000);

    expect(onFail).not.toHaveBeenCalled();
    expect(onError).not.toHaveBeenCalled();
    expect(streamFunction).toHaveBeenCalledTimes(1);
  });

  it("stops retrying after the retry budget is exhausted", async () => {
    const { streamFunction, last } = makeHarness();
    const onError = vi.fn();
    const onRestart = vi.fn();
    const stream = await createStream<number>(streamFunction, undefined, {
      onError,
      onRestart,
      retryAttempts: 3,
      retryDelay: 1000,
    });

    // three failures, three successful restarts
    for (let i = 0; i < 3; i++) {
      last().onFail();
      await vi.advanceTimersByTimeAsync(1000);
    }
    expect(streamFunction).toHaveBeenCalledTimes(4);
    expect(onRestart).toHaveBeenCalledTimes(3);

    // the budget is monotonic: the next failure is terminal
    last().onFail();
    await vi.advanceTimersByTimeAsync(10_000);

    expect(streamFunction).toHaveBeenCalledTimes(4);
    expect(onError).toHaveBeenCalledWith(expect.any(StreamFailedError));
    expect(stream.isDone).toBe(true);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("counts failed restart attempts against the retry budget", async () => {
    const instances: StreamInstance[] = [];
    const streamFunction = vi.fn(
      (callback: StreamCallback<number>, onFail: () => void) => {
        if (instances.length === 0) {
          const closer = makeCloser();
          instances.push({ callback, onFail, closer });
          return Promise.resolve(closer as unknown as StreamCloser);
        }
        // every restart attempt fails to create a stream
        return Promise.reject(new Error("creation failed"));
      },
    );
    const onError = vi.fn();
    const stream = await createStream<number>(streamFunction, undefined, {
      onError,
      retryAttempts: 2,
      retryDelay: 1000,
    });

    instances[0].onFail();
    await vi.advanceTimersByTimeAsync(10_000);

    // initial stream + two failed restart attempts
    expect(streamFunction).toHaveBeenCalledTimes(3);
    expect(onError).toHaveBeenCalledWith(expect.any(StreamFailedError));
    expect(stream.isDone).toBe(true);
  });

  it("restarts the stream after a failure and continues delivering values", async () => {
    const { instances, streamFunction, last } = makeHarness();
    const onValue = vi.fn();
    const onRestart = vi.fn();
    const onRetry = vi.fn();
    const stream = await createStream<number>(streamFunction, undefined, {
      onRestart,
      onRetry,
      onValue,
      retryDelay: 1000,
    });

    last().onFail();
    await vi.advanceTimersByTimeAsync(1000);

    expect(streamFunction).toHaveBeenCalledTimes(2);
    expect(onRestart).toHaveBeenCalledTimes(1);
    expect(onRetry).toHaveBeenCalledWith(1, 10);

    // values flow through the replacement stream
    last().callback(null, 42);
    expect(onValue).toHaveBeenCalledWith(42);

    // end() closes the replacement stream, not the original
    await stream.end();
    expect(instances[1].closer.end).toHaveBeenCalled();
  });

  it("invokes onEnd once when the stream ends", async () => {
    const { streamFunction } = makeHarness();
    const onEnd = vi.fn();
    const stream = await createStream<number>(streamFunction, undefined, {
      onEnd,
    });

    await stream.end();
    await stream.end();

    expect(onEnd).toHaveBeenCalledTimes(1);
  });

  it("ends the stream even when onError throws at terminal failure", async () => {
    const { instances, streamFunction } = makeHarness();
    const onError = vi.fn(() => {
      throw new Error("consumer onError threw");
    });
    const stream = await createStream<number>(streamFunction, undefined, {
      onError,
      retryAttempts: 0,
      retryDelay: 1000,
    });

    // the exhausted-budget native close triggers a terminal failure; a
    // throwing consumer onError must not prevent the stream from ending
    expect(() => instances[0].onFail()).toThrow("consumer onError threw");

    expect(onError).toHaveBeenCalledWith(expect.any(StreamFailedError));
    expect(stream.isDone).toBe(true);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("suppresses onValue when a sync mutator ends the stream", async () => {
    const { instances, streamFunction } = makeHarness();
    const onValue = vi.fn();
    const streamRef: { end?: () => Promise<unknown> } = {};
    const mutator = vi.fn((value: number) => {
      // a synchronous mutator that ends the stream, then returns a value
      void streamRef.end?.();
      return value * 2;
    });
    const stream = await createStream<number, number>(streamFunction, mutator, {
      onValue,
    });
    streamRef.end = () => stream.end();

    // drive a value after the proxy exists so the mutator can end the stream
    instances[0].callback(null, 1);

    expect(mutator).toHaveBeenCalled();
    expect(onValue).not.toHaveBeenCalled();
  });

  it("reschedules when the native stream closes during restart creation", async () => {
    const instances: StreamInstance[] = [];
    let resolveSecondReady!: () => void;
    const streamFunction = vi.fn(
      (callback: StreamCallback<number>, onFail: () => void) => {
        const closer = makeCloser();
        if (instances.length === 1) {
          // the restart stream holds waitForReady open so the test can close
          // it while its creation is still in flight
          closer.waitForReady = vi.fn(
            () =>
              new Promise<void>((resolve) => {
                resolveSecondReady = resolve;
              }),
          );
        }
        instances.push({ callback, onFail, closer });
        return Promise.resolve(closer as unknown as StreamCloser);
      },
    );
    const onRestart = vi.fn();
    await createStream<number>(streamFunction, undefined, {
      onRestart,
      retryDelay: 1000,
    });

    // the original stream closes and a retry is scheduled
    instances[0].onFail();
    await vi.advanceTimersByTimeAsync(1000);
    expect(streamFunction).toHaveBeenCalledTimes(2);

    // the replacement stream closes while its waitForReady is still pending
    instances[1].onFail();
    resolveSecondReady();
    await vi.advanceTimersByTimeAsync(0);

    // instead of installing the dead stream, the wrapper schedules a fresh
    // attempt; advancing past it opens a third native stream
    await vi.advanceTimersByTimeAsync(1000);
    expect(streamFunction).toHaveBeenCalledTimes(3);
    // the dead replacement was discarded, and only the live stream is announced
    expect(instances[1].closer.end).toHaveBeenCalled();
    expect(onRestart).toHaveBeenCalledTimes(1);
  });

  it("does not create a native stream when onRetry ends the stream", async () => {
    const { instances, streamFunction } = makeHarness();
    const streamRef: { end?: () => Promise<unknown> } = {};
    const onRetry = vi.fn(() => {
      void streamRef.end?.();
    });
    const stream = await createStream<number>(streamFunction, undefined, {
      onRetry,
      retryDelay: 1000,
    });
    streamRef.end = () => stream.end();

    // the native close schedules a retry; onRetry ends the stream when it fires
    instances[0].onFail();
    await vi.advanceTimersByTimeAsync(1000);

    expect(onRetry).toHaveBeenCalled();
    expect(streamFunction).toHaveBeenCalledTimes(1);
    expect(vi.getTimerCount()).toBe(0);
  });
});

describe("createStream", () => {
  it("should forward StreamFailedError to onError", async () => {
    const onErrorSpy = vi.fn();
    const onFailSpy = vi.fn();

    const mockStreamFunction = vi.fn(async (_, onFail: () => void) => {
      // Simulate immediate stream failure
      setTimeout(() => {
        onFail();
      }, 0);
      return Promise.resolve({
        end: vi.fn(),
        endAndWait: vi.fn().mockResolvedValue(undefined),
        isClosed: vi.fn().mockReturnValue(false),
        waitForReady: vi.fn().mockResolvedValue(undefined),
      });
    });

    const stream = await createStream(mockStreamFunction, undefined, {
      onError: onErrorSpy,
      onFail: onFailSpy,
      retryOnFail: false,
    });

    setTimeout(() => {
      void stream.end();
    }, 100);

    // Wait for the failure to be processed
    await new Promise((resolve) => setTimeout(resolve, 100));

    expect(onErrorSpy).toHaveBeenCalledWith(expect.any(StreamFailedError));
  });
});
