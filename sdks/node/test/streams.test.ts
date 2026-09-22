import type { StreamCloser } from "@xmtp/node-bindings";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { StreamFailedError } from "@/utils/errors";
import type { StreamFailureCause } from "@/utils/streamFailure";
import { createStream, type StreamCallback } from "@/utils/streams";

const makeCloser = () => ({
  end: vi.fn(),
  endAndWait: vi.fn().mockResolvedValue(undefined),
  isClosed: vi.fn().mockReturnValue(false),
  waitForReady: vi.fn().mockResolvedValue(undefined),
  catchUpSnapshot: vi.fn(() => null),
  catchUpChanged: vi.fn(async () => null),
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

  it.each(["none", "sync", "async"] as const)(
    "reports rejected onValue promises with %s mutation without ending or retrying",
    async (mode) => {
      const { streamFunction, last } = makeHarness();
      const error = new Error("Callback failed");
      const onError = vi.fn<(error: Error) => void>();
      const onEnd = vi.fn();
      const onFail = vi.fn();
      const onRetry = vi.fn();
      const onValue = vi
        .fn<(value: number) => Promise<void>>()
        .mockRejectedValueOnce(error)
        .mockResolvedValue(undefined);
      const mutator = (value: number) =>
        mode === "async" ? Promise.resolve(value * 2) : value * 2;
      const stream = await createStream<number>(
        streamFunction,
        mode === "none" ? undefined : mutator,
        { onEnd, onError, onFail, onRetry, onValue },
      );
      const firstValue = mode === "none" ? 1 : 2;

      try {
        last().callback(null, 1);
        if (mode === "async") {
          expect(onValue).not.toHaveBeenCalled();
        } else {
          expect(onValue).toHaveBeenCalledExactlyOnceWith(firstValue);
        }
        expect(onError).not.toHaveBeenCalled();

        await vi.advanceTimersByTimeAsync(0);
        expect(onError).toHaveBeenCalledExactlyOnceWith(error);
        expect(onError.mock.calls[0][0]).toBe(error);
        expect(stream.isDone).toBe(false);
        await expect(stream.next()).resolves.toEqual({
          done: false,
          value: firstValue,
        });

        last().callback(null, 2);
        await vi.advanceTimersByTimeAsync(0);
        await expect(stream.next()).resolves.toEqual({
          done: false,
          value: firstValue * 2,
        });
        expect(onValue.mock.calls).toEqual([[firstValue], [firstValue * 2]]);
        expect(onError).toHaveBeenCalledOnce();
        expect(onEnd).not.toHaveBeenCalled();
        expect(onFail).not.toHaveBeenCalled();
        expect(onRetry).not.toHaveBeenCalled();
        expect(last().closer.end).not.toHaveBeenCalled();
        expect(streamFunction).toHaveBeenCalledOnce();
        expect(vi.getTimerCount()).toBe(0);
      } finally {
        await stream.end();
      }
    },
  );

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
    expect(() => instances[0].onFail()).not.toThrow();

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

  it("keeps the live replacement when the first native stream closes before ready", async () => {
    const instances: StreamInstance[] = [];
    let resolveFirstReady!: () => void;
    const streamFunction = vi.fn(
      (callback: StreamCallback<number>, onFail: () => void) => {
        const closer = makeCloser();
        if (instances.length === 0) {
          closer.waitForReady = vi.fn(
            () =>
              new Promise<void>((resolve) => {
                resolveFirstReady = resolve;
              }),
          );
        }
        instances.push({ callback, onFail, closer });
        return Promise.resolve(closer as unknown as StreamCloser);
      },
    );
    const opening = createStream<number>(streamFunction, undefined, {
      retryDelay: 0,
    });
    await vi.advanceTimersByTimeAsync(0);
    expect(instances).toHaveLength(1);
    instances[0].onFail();
    await vi.advanceTimersByTimeAsync(0);
    expect(instances).toHaveLength(2);

    resolveFirstReady();
    const stream = await opening;
    await stream.end();
    expect(instances[1].closer.end).toHaveBeenCalledOnce();
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
      return Promise.resolve(makeCloser());
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

describe("createStream terminal native failures", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it.each([
    "LocalDeliveryError::NetworkRecoveryExhausted",
    "LocalDeliveryError::NetworkFailure",
    "ClientError::BackendMismatch",
    "ClientError::ClientVersionTooOld",
    "SubscribeError::Db",
    "SubscribeError::Storage",
    "GroupError::Db",
    "GroupError::Storage",
    "GroupError::SqlKeyStore",
    "ClientError::Db",
    "ClientError::Storage",
  ])("ends without retrying a native %s failure", async (code) => {
    const h = makeHarness();
    const onError = vi.fn();
    const stream = await createStream(h.streamFunction, undefined, {
      onError,
    });
    const pending = stream.next();
    const error = new Error(`[${code}] native cause`);
    const rejected = expect(pending).rejects.toBe(error);
    h.last().callback(error, undefined);
    h.last().onFail();
    await rejected;
    await expect(stream.next()).resolves.toEqual({
      done: true,
      value: undefined,
    });
    await vi.advanceTimersByTimeAsync(600_000);
    expect(onError).toHaveBeenCalledExactlyOnceWith(error);
    expect(h.streamFunction).toHaveBeenCalledOnce();
    expect(h.last().closer.end).toHaveBeenCalledOnce();
    expect(stream.isDone).toBe(true);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("opens a fresh stream after iterator rejection and fences the old completion", async () => {
    const h = makeHarness();
    const old = await createStream(h.streamFunction);
    const native = h.last();
    const cause = new Error(
      "[LocalDeliveryError::NetworkRecoveryExhausted] outage",
    );
    const pending = old.next();
    native.callback(cause, undefined);
    let replacement:
      | Awaited<ReturnType<typeof createStream<number>>>
      | undefined;
    try {
      await pending;
      throw new Error("the iterator must reject");
    } catch (error) {
      expect(error).toBe(cause);
      replacement = await createStream(h.streamFunction);
    }
    native.onFail();
    native.callback(null, 99);
    await old.end();
    h.last().callback(null, 42);
    expect(await replacement.next()).toEqual({ done: false, value: 42 });
    expect(h.streamFunction).toHaveBeenCalledTimes(2);
    expect(h.last().closer.end).not.toHaveBeenCalled();
    await replacement.end();
  });

  it("rejects the pending iterator when unexpected-close retries exhaust", async () => {
    const h = makeHarness();
    const stream = await createStream(h.streamFunction, undefined, {
      retryAttempts: 0,
    });
    const rejected = expect(stream.next()).rejects.toBeInstanceOf(
      StreamFailedError,
    );
    h.last().onFail();
    await rejected;
    expect(stream.isDone).toBe(true);
  });

  it.each([
    "LocalDeliveryError::NetworkRecoveryExhausted",
    "LocalDeliveryError::NetworkFailure",
    "SubscribeError::Db",
    "SubscribeError::Storage",
  ])(
    "opens a fresh stream inside onError after %s and fences late old callbacks",
    async (code) => {
      const h = makeHarness();
      let opening: ReturnType<typeof createStream<number>> | undefined;
      const onError = vi.fn(() => {
        expect(h.last().closer.end).toHaveBeenCalledOnce();
        opening = createStream(h.streamFunction, undefined, { onError });
      });
      let active = await createStream(h.streamFunction, undefined, { onError });
      for (let i = 0; i < 2; i++) {
        const old = active;
        const native = h.last();
        native.callback(new Error(`[${code}] native failure`), undefined);
        active = await opening!;
        native.onFail();
        native.callback(null, 99);
        await old.end();
        expect(old.isDone).toBe(true);
        expect(active.isDone).toBe(false);
        expect(h.last().closer.end).not.toHaveBeenCalled();
        h.last().callback(null, i);
        expect(await active.next()).toEqual({ done: false, value: i });
      }
      expect(onError).toHaveBeenCalledTimes(2);
      expect(h.streamFunction).toHaveBeenCalledTimes(3);
      await active.end();
    },
  );

  it.each([
    "LocalDeliveryError::NetworkRecoveryExhausted",
    "LocalDeliveryError::NetworkFailure",
    "ClientError::BackendMismatch",
    "ClientError::ClientVersionTooOld",
    "SubscribeError::Db",
    "SubscribeError::Storage",
    "GroupError::Db",
    "GroupError::Storage",
    "GroupError::SqlKeyStore",
    "ClientError::Db",
    "ClientError::Storage",
  ])(
    "does not retry a terminal %s rejection during native opening",
    async (code) => {
      const error = new Error(`[${code}] native opening failed`);
      const streamFunction = vi.fn().mockRejectedValue(error);
      const onError = vi.fn(async () => {
        throw new Error("error handler rejected");
      });
      // oxlint-disable-next-line typescript/no-misused-promises -- Verify that an async handler rejection preserves the original cause.
      const stream = await createStream(streamFunction, undefined, { onError });
      await expect(stream.next()).rejects.toBe(error);
      await vi.advanceTimersByTimeAsync(600_000);
      expect(streamFunction).toHaveBeenCalledOnce();
      expect(onError).toHaveBeenCalledExactlyOnceWith(error);
      expect(vi.getTimerCount()).toBe(0);
    },
  );

  it.each([
    "LocalDeliveryError::NetworkRecoveryExhausted",
    "LocalDeliveryError::NetworkFailure",
    "ClientError::BackendMismatch",
    "ClientError::ClientVersionTooOld",
    "SubscribeError::Db",
    "SubscribeError::Storage",
  ])(
    "ends on a %s callback before initial native creation completes",
    async (code) => {
      const error = new Error(`[${code}] initial storage query failed`);
      const closer = makeCloser();
      const onError = vi.fn();
      const streamFunction = vi.fn(async (callback: StreamCallback<number>) => {
        callback(error, undefined);
        return closer as unknown as StreamCloser;
      });
      const stream = await createStream(streamFunction, undefined, { onError });
      await expect(stream.next()).rejects.toBe(error);
      await vi.advanceTimersByTimeAsync(600_000);
      expect(onError).toHaveBeenCalledExactlyOnceWith(error);
      expect(streamFunction).toHaveBeenCalledOnce();
      expect(closer.end).toHaveBeenCalledOnce();
      expect(stream.isDone).toBe(true);
      expect(vi.getTimerCount()).toBe(0);
    },
  );

  it("ignores an old native close after an internal replacement", async () => {
    const h = makeHarness();
    const stream = await createStream(h.streamFunction, undefined, {
      retryDelay: 10,
    });
    h.instances[0].onFail();
    await vi.advanceTimersByTimeAsync(10);
    h.instances[0].onFail();
    await vi.advanceTimersByTimeAsync(10);
    expect(h.streamFunction).toHaveBeenCalledTimes(2);
    h.last().callback(null, 7);
    expect(await stream.next()).toEqual({ done: false, value: 7 });
    await stream.end();
  });

  it.each([
    "LocalDeliveryError::NetworkRecoveryExhausted",
    "LocalDeliveryError::NetworkFailure",
    "ClientError::BackendMismatch",
    "ClientError::ClientVersionTooOld",
    "SubscribeError::Db",
    "SubscribeError::Storage",
  ])("ends if a fallback opening fails with %s", async (code) => {
    const h = makeHarness();
    const onError = vi.fn();
    const stream = await createStream(h.streamFunction, undefined, {
      retryDelay: 10,
      onError,
    });
    const error = new Error(`[${code}] storage query failed`);
    h.streamFunction.mockRejectedValueOnce(error);
    const rejected = expect(stream.next()).rejects.toBe(error);
    h.last().onFail();
    await vi.advanceTimersByTimeAsync(10);
    await rejected;
    await vi.advanceTimersByTimeAsync(600_000);
    expect(onError).toHaveBeenCalledExactlyOnceWith(error);
    expect(h.streamFunction).toHaveBeenCalledTimes(2);
    expect(stream.isDone).toBe(true);
    expect(vi.getTimerCount()).toBe(0);
  });
});

const barrierError = (cause: Pick<StreamFailureCause, "kind" | "code">) =>
  new Error(
    `[GroupError::Barrier] receive failed\n[XMTP_STREAM_FAILURE_V1]${JSON.stringify(
      {
        kind: "barrier",
        code: "BarrierError::Deadline",
        message: "The receive barrier failed",
        retryable: true,
        intentId: null,
        publishedIntentIds: [],
        summary: null,
        barriers: [
          {
            reason: "deadline",
            unfinished: [
              {
                topic: "00ff",
                target: "1",
                received: "0",
                processed: "0",
                unresolvedWelcomes: [],
                inactive: false,
                cause: {
                  ...cause,
                  message: "The operation failed",
                  retryable: true,
                },
              },
            ],
          },
        ],
      },
    )}`,
  );

describe.each(["sync", "async"] as const)(
  "%s notification conversion failures",
  (mode) => {
    beforeEach(() => vi.useFakeTimers());
    afterEach(() => vi.useRealTimers());

    it.each([
      ["metadata DB", () => new Error("[GroupError::Db] query failed")],
      [
        "metadata storage",
        () => new Error("[GroupError::Storage] query failed"),
      ],
      [
        "metadata key store",
        () => new Error("[GroupError::SqlKeyStore] transaction failed"),
      ],
      [
        "barrier storage",
        () => barrierError({ kind: "storage", code: "ConnectionError" }),
      ],
      [
        "receiver storage",
        () => barrierError({ kind: "receiver", code: "incoming_storage" }),
      ],
    ] as const)(
      "ends on %s and keeps the original cause",
      async (_, makeError) => {
        const h = makeHarness();
        const cause = makeError();
        const onError = vi.fn();
        const onValue = vi.fn();
        const mutate = vi.fn<(value: number) => number | Promise<number>>(
          () => {
            if (mode === "async") return Promise.reject(cause);
            throw cause;
          },
        );
        const stream = await createStream(h.streamFunction, mutate, {
          onError,
          onValue,
        });
        const rejected = expect(stream.next()).rejects.toBe(cause);
        h.last().callback(null, 1);
        await rejected;
        h.last().onFail();
        h.last().callback(null, 2);
        await vi.advanceTimersByTimeAsync(600_000);
        expect(onError).toHaveBeenCalledExactlyOnceWith(cause);
        expect(onValue).not.toHaveBeenCalled();
        expect(mutate).toHaveBeenCalledOnce();
        expect(h.last().closer.end).toHaveBeenCalledOnce();
        expect(h.streamFunction).toHaveBeenCalledOnce();
        expect(stream.isDone).toBe(true);
        expect(vi.getTimerCount()).toBe(0);
      },
    );

    it.each([
      ["codec error", () => new Error("The application codec failed")],
      [
        "nonstorage receiver",
        () => barrierError({ kind: "receiver", code: "incoming_receive" }),
      ],
    ] as const)("reports %s and permits later values", async (_, makeError) => {
      const h = makeHarness();
      const cause = makeError();
      const onError = vi.fn();
      const mutate = vi.fn<(value: number) => number | Promise<number>>(
        (value) => value,
      );
      mutate.mockImplementationOnce(() => {
        if (mode === "async") return Promise.reject(cause);
        throw cause;
      });
      const stream = await createStream(h.streamFunction, mutate, { onError });
      h.last().callback(null, 1);
      await vi.advanceTimersByTimeAsync(0);
      expect(onError).toHaveBeenCalledExactlyOnceWith(cause);
      expect(stream.isDone).toBe(false);
      h.last().callback(null, 2);
      await expect(stream.next()).resolves.toEqual({ done: false, value: 2 });
      expect(h.streamFunction).toHaveBeenCalledOnce();
      expect(h.last().closer.end).not.toHaveBeenCalled();
      await stream.end();
    });
  },
);

describe("structured notification startup failure", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it.each([
    { kind: "storage", code: "ConnectionError" },
    { kind: "receiver", code: "incoming_storage" },
  ] as const)("does not retry a pre-sync $kind failure", async (details) => {
    const cause = barrierError(details);
    const streamFunction = vi.fn().mockRejectedValue(cause);
    const onError = vi.fn();
    const stream = await createStream(streamFunction, undefined, { onError });
    await expect(stream.next()).rejects.toBe(cause);
    await vi.advanceTimersByTimeAsync(600_000);
    expect(onError).toHaveBeenCalledExactlyOnceWith(cause);
    expect(streamFunction).toHaveBeenCalledOnce();
    expect(stream.isDone).toBe(true);
  });

  it.each(["none", "sync"] as const)(
    "retains an accepted onValue storage failure across reopen with %s mutation",
    async (mode) => {
      const h = makeHarness();
      const pending = Promise.withResolvers<undefined>();
      const onError = vi.fn();
      const onValue = vi.fn().mockReturnValueOnce(pending.promise);
      const stream = await createStream(
        h.streamFunction,
        mode === "sync" ? (value) => value : undefined,
        {
          onValue,
          onError,
          retryDelay: 10,
        },
      );
      const old = h.last();
      old.callback(null, 1);
      await expect(stream.next()).resolves.toEqual({ done: false, value: 1 });
      old.onFail();
      await vi.advanceTimersByTimeAsync(10);
      const cause = new Error("[GroupError::Storage] accepted callback failed");
      pending.reject(cause);
      await vi.advanceTimersByTimeAsync(0);
      expect(onError).toHaveBeenCalledExactlyOnceWith(cause);
      expect(stream.isDone).toBe(true);
      expect(h.last().closer.end).toHaveBeenCalledOnce();
      await expect(stream.next()).rejects.toBe(cause);
      await expect(stream.next()).resolves.toEqual({
        done: true,
        value: undefined,
      });
      await stream.end();
    },
  );
});

describe("accepted notification conversion", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("delivers an accepted value when conversion finishes after an internal reopen", async () => {
    const h = makeHarness();
    const conversion = Promise.withResolvers<number>();
    const onValue = vi.fn();
    const stream = await createStream(
      h.streamFunction,
      () => conversion.promise,
      {
        onValue,
        retryDelay: 10,
      },
    );
    const received = stream.next();
    const old = h.last();
    old.callback(null, 1);
    old.onFail();
    await vi.advanceTimersByTimeAsync(10);
    expect(h.streamFunction).toHaveBeenCalledTimes(2);
    conversion.resolve(1);
    await expect(received).resolves.toEqual({ done: false, value: 1 });
    expect(onValue).toHaveBeenCalledExactlyOnceWith(1);
    await stream.end();
  });

  it("keeps the rejection on the pending read when onError reads the old handle", async () => {
    const h = makeHarness();
    let reentrant:
      | ReturnType<Awaited<ReturnType<typeof createStream>>["next"]>
      | undefined;
    const stream = await createStream(h.streamFunction, undefined, {
      onError: () => {
        reentrant = stream.next();
      },
    });
    const cause = new Error("[SubscribeError::Storage] failed read");
    const rejected = expect(stream.next()).rejects.toBe(cause);
    h.last().callback(cause, undefined);
    await rejected;
    await expect(reentrant).resolves.toEqual({ done: true, value: undefined });
  });

  it("discards resolved values whose JS handoff follows terminal failure", async () => {
    const h = makeHarness();
    const stream = await createStream(h.streamFunction);
    const settled = Promise.allSettled([stream.next(), stream.next()]);
    const native = h.last();
    native.callback(null, 1);
    native.callback(null, 2);
    const cause = new Error("[SubscribeError::Storage] failed read");
    native.callback(cause, undefined);
    expect(await settled).toEqual([
      { status: "rejected", reason: cause },
      { status: "fulfilled", value: { done: true, value: undefined } },
    ]);
  });

  it("rejects only one pending iterator read on terminal failure, then returns EOF", async () => {
    const h = makeHarness();
    const stream = await createStream(h.streamFunction);
    const reads = [stream.next(), stream.next()];
    const settled = Promise.allSettled(reads);
    const cause = new Error("[SubscribeError::Storage] failed read");
    h.last().callback(cause, undefined);
    expect(await settled).toEqual([
      { status: "rejected", reason: cause },
      { status: "fulfilled", value: { done: true, value: undefined } },
    ]);
    await expect(stream.next()).resolves.toEqual({
      done: true,
      value: undefined,
    });
    await expect(stream.return()).resolves.toEqual({
      done: true,
      value: undefined,
    });
  });
});

describe("createStream cleanup errors", () => {
  it.each(["end", "error"] as const)(
    "settles a pending iterator when close and onEnd throw during %s",
    async (termination) => {
      const h = makeHarness();
      const onEnd = vi.fn(() => {
        throw new Error("end handler failed");
      });
      const onError = vi.fn();
      const stream = await createStream(h.streamFunction, undefined, {
        onEnd,
        onError,
      });
      h.last().closer.end.mockImplementation(() => {
        throw new Error("native close failed");
      });
      const pending = stream.next();
      if (termination === "error") {
        const cause = new Error(
          "[SubscribeError::Storage] database unavailable",
        );
        const rejected = expect(pending).rejects.toBe(cause);
        expect(() => h.last().callback(cause, undefined)).not.toThrow();
        await rejected;
        expect(onError).toHaveBeenCalledExactlyOnceWith(cause);
      } else {
        await expect(stream.end()).resolves.toEqual({
          done: true,
          value: undefined,
        });
        await expect(pending).resolves.toEqual({
          done: true,
          value: undefined,
        });
        expect(onError).not.toHaveBeenCalled();
      }
      expect(onEnd).toHaveBeenCalledOnce();
      expect(stream.isDone).toBe(true);
      await stream.end();
      expect(onEnd).toHaveBeenCalledOnce();
    },
  );
});
