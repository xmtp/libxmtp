/**
 * Lifecycle contract of `createStream` (src/utils/streams.ts).
 *
 * `end()` is terminal. After a caller ends a stream, that wrapper must not
 * open another native stream, must not keep one open, and must not call any
 * of its callbacks again. Retries are single-flight and their budget is
 * bounded across consecutive failures.
 *
 * These tests use fake timers and a fake native layer. They load no native
 * bindings and use no network.
 *
 * The fake models `@xmtp/node-bindings` 1.10.0 (used by node-sdk 6.0.0, which
 * Agent SDK 2.3.0 pins) and 1.11.x (node-sdk 6.1.x):
 *
 * - `streamFunction(callback, onFail)` starts the native task at call time and
 *   resolves to a `StreamCloser` (bindings/node/src/conversations/streams.rs
 *   lines 35-64). The JS `onFail` is passed through as the native `on_close`
 *   (src/Conversations.ts lines 347-354).
 * - `on_close` is `FnOnce`. It runs when the native task ends by itself: the
 *   subscription ended, or it failed to start
 *   (crates/xmtp_mls/src/subscriptions/watchdog.rs lines 407-408, 433-436,
 *   492-493; router_callbacks.rs lines 429 and 577-579).
 * - `closer.end()` aborts the tokio task (bindings/node/src/streams.rs lines
 *   35-37, crates/xmtp_common/src/stream_handles.rs lines 265-268). An aborted
 *   task never reaches its `on_close` call (router_callbacks.rs lines 65 and
 *   501-502). The watchdog's cancellation branch (watchdog.rs lines 449 and
 *   463-464) listens to the client-wide token (subscriptions/mod.rs lines 500
 *   and 562), not to `closer.end()`.
 * - A customer report assumed that `closer.end()` also fires `on_close`.
 *   Every test runs under both models, and `createStream` must behave the
 *   same way under each.
 * - Native callbacks and `on_close` reach JS through napi ThreadsafeFunction
 *   queues. A call that was queued before `end()` can run after it.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { StreamCloser } from "@xmtp/node-bindings";
import type { AsyncStreamProxy } from "@/AsyncStream";
import { StreamFailedError } from "@/utils/errors";
import {
  createStream,
  type StreamCallback,
  type StreamFunction,
  type StreamOptions,
  type StreamValueMutator,
} from "@/utils/streams";

const RETRY_DELAY = 1_000;
const RETRY_ATTEMPTS = 3;

type Msg = { id: string };
const msg = (id: string): Msg => ({ id });

/** One native subscription task, as the bindings start it. */
type FakeNativeStream = {
  id: number;
  /** The JS callback the native call captured. */
  callback: StreamCallback<Msg>;
  /** The JS `onFail` the native call captured as `on_close`. No guard. */
  onClose: () => void;
  /** True while the native task runs. */
  active: boolean;
  /** `on_close` is FnOnce: true once it has reached JS. */
  closeDelivered: boolean;
  closer: StreamCloser;
};

type NativeModel = {
  label: string;
  endFiresOnClose: boolean;
};

const NATIVE_MODELS: NativeModel[] = [
  {
    label:
      "native end() aborts without on_close (node-bindings 1.10.0 and 1.11.x)",
    endFiresOnClose: false,
  },
  {
    label: "native end() also delivers on_close (the bug report's model)",
    endFiresOnClose: true,
  },
];

type Held = PromiseWithResolvers<undefined>;

const releaser = (held: Held) => ({
  release: () => {
    held.resolve(undefined);
  },
});

const createFakeNative = ({ endFiresOnClose }: NativeModel) => {
  const streams: FakeNativeStream[] = [];
  let heldCreation: Held | undefined;
  let heldReady: Held | undefined;
  let creationError: Error | undefined;
  let creationFailures = 0;

  /** The native task ended by itself. Its `on_close` reaches JS, once. */
  const deliverClose = (stream: FakeNativeStream) => {
    stream.active = false;
    if (stream.closeDelivered) return;
    stream.closeDelivered = true;
    stream.onClose();
  };

  const streamFunction = vi.fn<StreamFunction<Msg>>(
    async (callback, onFail) => {
      if (creationError && creationFailures > 0) {
        creationFailures -= 1;
        throw creationError;
      }
      const creation = heldCreation;
      const ready = heldReady;
      heldCreation = undefined;
      heldReady = undefined;

      const stream: FakeNativeStream = {
        id: streams.length + 1,
        callback,
        onClose: onFail,
        active: true,
        closeDelivered: false,
        closer: {
          end: () => {
            if (!stream.active) return;
            stream.active = false;
            if (endFiresOnClose) {
              // on_close would run on a tokio thread and reach JS through
              // the ThreadsafeFunction queue, never inside end() itself.
              setTimeout(() => {
                deliverClose(stream);
              }, 0);
            }
          },
          endAndWait: () => {
            stream.closer.end();
            return Promise.resolve();
          },
          waitForReady: () => ready?.promise ?? Promise.resolve(),
          isClosed: () => !stream.active,
        },
      };
      // The native task starts before the JS promise resolves.
      streams.push(stream);
      if (creation) await creation.promise;
      return stream.closer;
    },
  );

  return {
    streams,
    streamFunction,
    deliverClose,
    /** The next `streamFunction` call returns its closer only on release. */
    holdNextCreation: () => {
      heldCreation = Promise.withResolvers<undefined>();
      return releaser(heldCreation);
    },
    /** The next native stream reports ready only on release. */
    holdNextReady: () => {
      heldReady = Promise.withResolvers<undefined>();
      return releaser(heldReady);
    },
    /** The next `times` calls to `streamFunction` reject with `error`. */
    failCreations: (error: Error, times = Number.POSITIVE_INFINITY) => {
      creationError = error;
      creationFailures = times;
    },
    /** A native callback reaches JS. It may have been queued before end(). */
    deliverValue: (stream: FakeNativeStream, value: Msg) => {
      stream.callback(null, value);
    },
    /** A native error item reaches JS. */
    deliverError: (stream: FakeNativeStream, error: Error) => {
      stream.callback(error, undefined);
    },
    /** The server pushes one message to every live subscription. */
    emitToActive: (value: Msg) => {
      for (const stream of streams.filter((s) => s.active)) {
        stream.callback(null, value);
      }
    },
    /** Ids of the native streams that are still running. */
    active: () => streams.filter((s) => s.active).map((s) => s.id),
  };
};

type FakeNative = ReturnType<typeof createFakeNative>;

const createSpies = () => ({
  onValue: vi.fn<(value: Msg) => void>(),
  onError: vi.fn<(error: Error) => void>(),
  onFail: vi.fn<() => void>(),
  onRetry: vi.fn<(attempts: number, maxAttempts: number) => void>(),
  onRestart: vi.fn<() => void>(),
});

type Spies = ReturnType<typeof createSpies>;

const open = (
  fake: FakeNative,
  spies: Spies,
  mutator?: StreamValueMutator<Msg, Msg | undefined>,
  options?: StreamOptions<Msg, Msg>,
) =>
  createStream<Msg, Msg>(fake.streamFunction, mutator, {
    ...spies,
    retryAttempts: RETRY_ATTEMPTS,
    retryDelay: RETRY_DELAY,
    ...options,
  });

/** What one wrapper did, in a shape that diffs well when a test fails. */
const observe = (fake: FakeNative, spies: Spies) => ({
  streamFunctionCalls: fake.streamFunction.mock.calls.length,
  activeNativeStreams: fake.active(),
  onValue: spies.onValue.mock.calls.length,
  onError: spies.onError.mock.calls.length,
  onFail: spies.onFail.mock.calls.length,
  onRetry: spies.onRetry.mock.calls,
  onRestart: spies.onRestart.mock.calls.length,
});

describe.each(NATIVE_MODELS)("createStream lifecycle: $label", (model) => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe("end() is terminal", () => {
    it("end() during a pending retry cancels the retry", async () => {
      const fake = createFakeNative(model);
      const spies = createSpies();
      const stream = await open(fake, spies);
      const [first] = fake.streams;

      // The native stream ends by itself, so the wrapper schedules a retry.
      fake.deliverClose(first);
      // The caller ends the wrapper before the retry delay expires.
      expect(vi.getTimerCount()).toBe(1);
      await stream.end();
      // The retry timer is cleared, not left to fire and be ignored.
      expect(vi.getTimerCount()).toBe(0);
      await vi.advanceTimersByTimeAsync(RETRY_DELAY * 5);

      expect(observe(fake, spies)).toMatchObject({
        streamFunctionCalls: 1,
        activeNativeStreams: [],
        onFail: 1,
        onRetry: [],
        onRestart: 0,
      });
    });

    it("end() from inside onRetry opens no native stream", async () => {
      const fake = createFakeNative(model);
      const spies = createSpies();
      const stream = await open(fake, spies);
      spies.onRetry.mockImplementation(() => {
        void stream.end();
      });

      fake.deliverClose(fake.streams[0]!);
      await vi.advanceTimersByTimeAsync(RETRY_DELAY * 5);

      expect(observe(fake, spies)).toMatchObject({
        streamFunctionCalls: 1,
        activeNativeStreams: [],
        onRetry: [[1, RETRY_ATTEMPTS]],
        onRestart: 0,
      });
    });

    it("an intentional end() never schedules a retry", async () => {
      const fake = createFakeNative(model);
      const spies = createSpies();
      const stream = await open(fake, spies);

      await stream.end();
      await vi.advanceTimersByTimeAsync(RETRY_DELAY * 5);

      // A close the caller asked for is not a failure.
      expect(observe(fake, spies)).toMatchObject({
        streamFunctionCalls: 1,
        activeNativeStreams: [],
        onError: 0,
        onFail: 0,
        onRetry: [],
        onRestart: 0,
      });
    });

    it("a native close that reaches JS after end() is not a failure", async () => {
      const fake = createFakeNative(model);
      const spies = createSpies();
      const stream = await open(fake, spies);
      const [first] = fake.streams;

      // The native task ended by itself and queued on_close, but the caller
      // called end() before JS ran that queued on_close.
      await stream.end();
      fake.deliverClose(first);
      await vi.advanceTimersByTimeAsync(RETRY_DELAY * 5);

      expect(observe(fake, spies)).toMatchObject({
        streamFunctionCalls: 1,
        activeNativeStreams: [],
        onFail: 0,
        onRetry: [],
        onRestart: 0,
      });
    });

    it("end() while a retry's streamFunction is in flight ends the late closer and skips onRestart", async () => {
      const fake = createFakeNative(model);
      const spies = createSpies();
      const stream = await open(fake, spies);
      const [first] = fake.streams;

      const creation = fake.holdNextCreation();
      fake.deliverClose(first);
      await vi.advanceTimersByTimeAsync(RETRY_DELAY);
      // The retry is inside streamFunction: the native task already runs,
      // but JS does not have its closer yet.
      expect(fake.streamFunction).toHaveBeenCalledTimes(2);

      await stream.end();
      creation.release();
      await vi.advanceTimersByTimeAsync(0);

      // The closer that arrived after end() is ended at once.
      expect(observe(fake, spies)).toMatchObject({
        activeNativeStreams: [],
        onRestart: 0,
      });

      // Its close does not start another retry.
      await vi.advanceTimersByTimeAsync(RETRY_DELAY * 5);
      expect(observe(fake, spies)).toMatchObject({
        streamFunctionCalls: 2,
        activeNativeStreams: [],
        onRetry: [[1, RETRY_ATTEMPTS]],
        onRestart: 0,
      });
    });

    // createStream returns the stream only after the first native stream is
    // ready, so a caller cannot call end() during the first creation. The
    // nearest reachable case: the first attempt fails, createStream returns
    // a stream in its retry state, and end() lands while that retry waits
    // for ready.
    it("end() while a retry's waitForReady is pending ends that closer and delivers nothing", async () => {
      const fake = createFakeNative(model);
      const spies = createSpies();
      const connectError = new Error("connect failed");
      fake.failCreations(connectError, 1);

      const stream = await open(fake, spies);
      expect(spies.onError).toHaveBeenCalledWith(connectError);
      expect(fake.streams).toHaveLength(0);

      const ready = fake.holdNextReady();
      await vi.advanceTimersByTimeAsync(RETRY_DELAY);
      expect(fake.streamFunction).toHaveBeenCalledTimes(2);
      const [retried] = fake.streams;

      await stream.end();
      ready.release();
      await vi.advanceTimersByTimeAsync(0);
      fake.deliverValue(retried, msg("late"));
      await vi.advanceTimersByTimeAsync(RETRY_DELAY * 5);

      expect(observe(fake, spies)).toMatchObject({
        streamFunctionCalls: 2,
        activeNativeStreams: [],
        onValue: 0,
        onError: 1,
        onRestart: 0,
      });
    });

    it("calls neither onValue nor onError after end(), even for native callbacks queued before it", async () => {
      const fake = createFakeNative(model);
      const spies = createSpies();
      const stream = await open(fake, spies);
      const [first] = fake.streams;

      fake.deliverValue(first, msg("before-end"));
      await stream.end();
      fake.deliverValue(first, msg("after-end"));
      fake.deliverError(first, new Error("h2 protocol error"));
      await vi.advanceTimersByTimeAsync(0);

      expect(spies.onValue.mock.calls).toEqual([[msg("before-end")]]);
      expect(spies.onError).not.toHaveBeenCalled();
    });

    it("an async value mutator that settles after end() delivers nothing", async () => {
      const fake = createFakeNative(model);
      const spies = createSpies();
      const decodes = [
        Promise.withResolvers<Msg>(),
        Promise.withResolvers<Msg>(),
      ];
      let decodeCall = 0;
      const mutator = vi.fn((_value: Msg) => decodes[decodeCall++]!.promise);
      const stream = await open(fake, spies, mutator);
      const [first] = fake.streams;

      fake.deliverValue(first, msg("m1"));
      fake.deliverValue(first, msg("m2"));
      expect(mutator).toHaveBeenCalledTimes(2);

      await stream.end();
      decodes[0]!.resolve(msg("m1-decoded"));
      decodes[1]!.reject(new Error("decode failed"));
      await vi.advanceTimersByTimeAsync(0);

      expect(observe(fake, spies)).toMatchObject({ onValue: 0, onError: 0 });
    });
  });

  describe("retry policy", () => {
    // Native on_close is FnOnce (watchdog.rs lines 376 and 424), so one
    // native stream closes once. A burst reaches one wrapper when it owns
    // several native streams at once, or when close notifications queue up.
    it("retry is single-flight: a burst of close notifications schedules one replacement", async () => {
      const fake = createFakeNative(model);
      const spies = createSpies();
      await open(fake, spies);
      const [first] = fake.streams;

      first.active = false;
      first.onClose();
      first.onClose();
      first.onClose();
      await vi.advanceTimersByTimeAsync(RETRY_DELAY);

      expect(observe(fake, spies)).toMatchObject({
        streamFunctionCalls: 2,
        activeNativeStreams: [2],
        onRetry: [[1, RETRY_ATTEMPTS]],
        onRestart: 1,
      });
    });

    it("retry is single-flight: a close while a retry is in flight does not start a second one", async () => {
      const fake = createFakeNative(model);
      const spies = createSpies();
      await open(fake, spies);
      const [first] = fake.streams;

      const creation = fake.holdNextCreation();
      first.active = false;
      first.onClose();
      await vi.advanceTimersByTimeAsync(RETRY_DELAY);
      expect(fake.streamFunction).toHaveBeenCalledTimes(2);

      first.onClose();
      creation.release();
      await vi.advanceTimersByTimeAsync(RETRY_DELAY * 5);

      expect(observe(fake, spies)).toMatchObject({
        streamFunctionCalls: 2,
        activeNativeStreams: [2],
        onRetry: [[1, RETRY_ATTEMPTS]],
        onRestart: 1,
      });
    });

    it("consecutive failed creations stop after retryAttempts", async () => {
      const fake = createFakeNative(model);
      const spies = createSpies();
      const stream = await open(fake, spies);
      const [first] = fake.streams;

      fake.failCreations(new Error("connect failed"));
      fake.deliverClose(first);
      await vi.advanceTimersByTimeAsync(RETRY_DELAY * (RETRY_ATTEMPTS + 5));

      expect(observe(fake, spies)).toMatchObject({
        streamFunctionCalls: 1 + RETRY_ATTEMPTS,
        activeNativeStreams: [],
        onRetry: [
          [1, RETRY_ATTEMPTS],
          [2, RETRY_ATTEMPTS],
          [3, RETRY_ATTEMPTS],
        ],
      });
      expect(spies.onError).toHaveBeenLastCalledWith(
        expect.any(StreamFailedError),
      );
      expect(stream.isDone).toBe(true);
    });

    // A native stream that fails to start calls on_close and then reports
    // ready (watchdog.rs lines 431-437; the ready sender is dropped,
    // stream_handles.rs lines 234-237). That start is a failed attempt: it
    // spends the budget and is not a restart.
    it("a native stream that closes before it is ready spends the budget", async () => {
      const fake = createFakeNative(model);
      const spies = createSpies();
      const stream = await open(fake, spies);
      const [first] = fake.streams;

      fake.deliverClose(first);
      for (let i = 0; i < RETRY_ATTEMPTS + 3; i++) {
        const ready = fake.holdNextReady();
        await vi.advanceTimersByTimeAsync(RETRY_DELAY);
        const starting = fake.streams.at(-1)!;
        if (!starting.active) break;
        fake.deliverClose(starting);
        ready.release();
        await vi.advanceTimersByTimeAsync(0);
      }
      await vi.advanceTimersByTimeAsync(RETRY_DELAY * 5);

      expect(observe(fake, spies)).toMatchObject({
        streamFunctionCalls: 1 + RETRY_ATTEMPTS,
        activeNativeStreams: [],
        onFail: 1 + RETRY_ATTEMPTS,
        onRetry: [
          [1, RETRY_ATTEMPTS],
          [2, RETRY_ATTEMPTS],
          [3, RETRY_ATTEMPTS],
        ],
        onRestart: 0,
      });
      expect(spies.onError.mock.calls).toEqual([
        [expect.any(StreamFailedError)],
      ]);
      expect(stream.isDone).toBe(true);
    });

    // The same failed start, when on_close has not reached JS yet but the
    // native task already finished.
    it("a native stream that is already closed when it reports ready spends the budget", async () => {
      const fake = createFakeNative(model);
      const spies = createSpies();
      const stream = await open(fake, spies);
      const [first] = fake.streams;

      fake.deliverClose(first);
      for (let i = 0; i < RETRY_ATTEMPTS + 3; i++) {
        const ready = fake.holdNextReady();
        await vi.advanceTimersByTimeAsync(RETRY_DELAY);
        const starting = fake.streams.at(-1)!;
        if (!starting.active) break;
        starting.active = false;
        ready.release();
        await vi.advanceTimersByTimeAsync(0);
      }
      await vi.advanceTimersByTimeAsync(RETRY_DELAY * 5);

      expect(observe(fake, spies)).toMatchObject({
        streamFunctionCalls: 1 + RETRY_ATTEMPTS,
        activeNativeStreams: [],
        onRetry: [
          [1, RETRY_ATTEMPTS],
          [2, RETRY_ATTEMPTS],
          [3, RETRY_ATTEMPTS],
        ],
        onRestart: 0,
      });
      expect(spies.onError).toHaveBeenLastCalledWith(
        expect.any(StreamFailedError),
      );
      expect(stream.isDone).toBe(true);
    });

    // An on_close can reach JS while streamFunction is still resolving.
    it("a native stream that closes before streamFunction resolves is a failed attempt", async () => {
      const fake = createFakeNative(model);
      const spies = createSpies();
      await open(fake, spies);
      const [first] = fake.streams;

      const creation = fake.holdNextCreation();
      fake.deliverClose(first);
      await vi.advanceTimersByTimeAsync(RETRY_DELAY);
      const starting = fake.streams[1]!;
      fake.deliverClose(starting);
      creation.release();
      await vi.advanceTimersByTimeAsync(RETRY_DELAY);

      expect(observe(fake, spies)).toMatchObject({
        streamFunctionCalls: 3,
        activeNativeStreams: [3],
        onFail: 2,
        onRetry: [
          [1, RETRY_ATTEMPTS],
          [2, RETRY_ATTEMPTS],
        ],
        onRestart: 1,
      });
    });

    // Policy (unchanged from earlier releases): a restart whose native
    // stream reported ready is a success, so a later close starts a new
    // retry sequence with the full budget. Such a stream retries once per
    // retryDelay for as long as it keeps coming up.
    it("a restart that reported ready restores the full budget", async () => {
      const fake = createFakeNative(model);
      const spies = createSpies();
      const stream = await open(fake, spies);

      for (let i = 0; i < RETRY_ATTEMPTS + 2; i++) {
        fake.deliverClose(fake.streams.at(-1)!);
        await vi.advanceTimersByTimeAsync(RETRY_DELAY);
      }

      expect(observe(fake, spies)).toMatchObject({
        streamFunctionCalls: RETRY_ATTEMPTS + 3,
        activeNativeStreams: [RETRY_ATTEMPTS + 3],
        onError: 0,
        onRestart: RETRY_ATTEMPTS + 2,
      });
      expect(spies.onRetry.mock.calls).toEqual(
        Array.from({ length: RETRY_ATTEMPTS + 2 }, () => [1, RETRY_ATTEMPTS]),
      );
      expect(stream.isDone).toBe(false);
    });

    it("an onRestart that throws is reported and does not start a second retry", async () => {
      const fake = createFakeNative(model);
      const spies = createSpies();
      const restartError = new Error("onRestart failed");
      spies.onRestart.mockImplementation(() => {
        throw restartError;
      });
      await open(fake, spies);

      fake.deliverClose(fake.streams[0]!);
      await vi.advanceTimersByTimeAsync(RETRY_DELAY * 5);

      expect(observe(fake, spies)).toMatchObject({
        streamFunctionCalls: 2,
        activeNativeStreams: [2],
        onRestart: 1,
      });
      expect(spies.onError.mock.calls).toEqual([[restartError]]);
    });

    // Policy: once a replacement has proved healthy (it delivered a value
    // and stayed up for many retry delays), a later failure is a new
    // incident and gets the full budget again. A lifetime cap would
    // eventually kill the stream of a long-running agent.
    it("a replacement that proved healthy restores the full budget", async () => {
      const fake = createFakeNative(model);
      const spies = createSpies();
      const stream = await open(fake, spies);
      const [first] = fake.streams;

      fake.deliverClose(first);
      await vi.advanceTimersByTimeAsync(RETRY_DELAY);
      const replacement = fake.streams[1]!;
      fake.deliverValue(replacement, msg("m1"));
      await vi.advanceTimersByTimeAsync(RETRY_DELAY * 10);

      fake.failCreations(new Error("connect failed"));
      fake.deliverClose(replacement);
      await vi.advanceTimersByTimeAsync(RETRY_DELAY * (RETRY_ATTEMPTS + 5));

      expect(observe(fake, spies)).toMatchObject({
        streamFunctionCalls: 2 + RETRY_ATTEMPTS,
        activeNativeStreams: [],
        onValue: 1,
        onRetry: [
          [1, RETRY_ATTEMPTS],
          [1, RETRY_ATTEMPTS],
          [2, RETRY_ATTEMPTS],
          [3, RETRY_ATTEMPTS],
        ],
      });
      expect(spies.onError).toHaveBeenLastCalledWith(
        expect.any(StreamFailedError),
      );
      expect(stream.isDone).toBe(true);
    });
  });

  describe("retryOnFail: false", () => {
    it("an unexpected native close ends the stream with one StreamFailedError", async () => {
      const fake = createFakeNative(model);
      const spies = createSpies();
      const stream = await open(fake, spies, undefined, {
        retryOnFail: false,
      });

      fake.deliverClose(fake.streams[0]!);
      await vi.advanceTimersByTimeAsync(RETRY_DELAY * 5);

      expect(observe(fake, spies)).toMatchObject({
        streamFunctionCalls: 1,
        activeNativeStreams: [],
        onFail: 1,
        onRetry: [],
        onRestart: 0,
      });
      expect(spies.onError.mock.calls).toEqual([[new StreamFailedError(0)]]);
      expect(stream.isDone).toBe(true);
    });

    it("a caller's end() reports no StreamFailedError", async () => {
      const fake = createFakeNative(model);
      const spies = createSpies();
      const stream = await open(fake, spies, undefined, {
        retryOnFail: false,
      });

      await stream.end();
      fake.deliverClose(fake.streams[0]!);
      await vi.advanceTimersByTimeAsync(RETRY_DELAY * 5);

      expect(observe(fake, spies)).toMatchObject({
        streamFunctionCalls: 1,
        activeNativeStreams: [],
        onError: 0,
        onFail: 0,
      });
    });
  });

  // Agent SDK 2.3.0 owns recovery around these streams: onError ends the
  // current stream and opens a replacement, and it leaves retryOnFail at its
  // default (xmtp-js 8ee0d18, sdks/agent-sdk/src/core/Agent.ts lines 318-371
  // and 373-426).
  describe("Agent SDK recovery", () => {
    it("reproduces the report: an ended wrapper's retry must not open a second live stream", async () => {
      const fake = createFakeNative(model);
      const delivered: string[] = [];
      const options = {
        retryAttempts: RETRY_ATTEMPTS,
        retryDelay: RETRY_DELAY,
      };

      // 1. Wrapper A opens native stream 1.
      const streamA = await createStream<Msg, Msg>(
        fake.streamFunction,
        undefined,
        { ...options, onValue: (value) => delivered.push(`old:${value.id}`) },
      );
      // 2. Native stream 1 closes, so A schedules its retry.
      fake.deliverClose(fake.streams[0]!);
      // 3. The owner ends A before A's retry delay expires.
      await streamA.end();
      // 4. The owner opens replacement wrapper B on native stream 2.
      await createStream<Msg, Msg>(fake.streamFunction, undefined, {
        ...options,
        onValue: (value) => delivered.push(`current:${value.id}`),
      });
      // 5. A's retry timer expires.
      await vi.advanceTimersByTimeAsync(RETRY_DELAY * 5);
      // 6. The server pushes one message to every live subscription.
      fake.emitToActive(msg("same-message-id"));

      expect({
        wrapperAEnded: streamA.isDone,
        activeNativeStreams: fake.active(),
        delivered,
      }).toEqual({
        wrapperAEnded: true,
        activeNativeStreams: [2],
        delivered: ["current:same-message-id"],
      });
    });

    it("repeated recovery keeps one live stream and one callback per message", async () => {
      const fake = createFakeNative(model);
      const delivered: string[] = [];
      let current: AsyncStreamProxy<Msg> | undefined;
      let currentNative: FakeNativeStream | undefined;
      let restarting = false;

      const start = async () => {
        const index = fake.streams.length;
        current = await createStream<Msg, Msg>(fake.streamFunction, undefined, {
          retryAttempts: RETRY_ATTEMPTS,
          retryDelay: RETRY_DELAY,
          onValue: (value) => delivered.push(value.id),
          onError: () => void recover(),
        });
        currentNative = fake.streams[index];
      };
      const recover = async () => {
        if (restarting) return;
        restarting = true;
        await current?.end();
        current = undefined;
        await start();
        restarting = false;
      };
      await start();

      for (let cycle = 0; cycle < 5; cycle++) {
        const native = currentNative!;
        // A transport error: the native stream forwards the error item and
        // its subscription ends. JS runs onError (the owner ends the stream
        // and opens a replacement), then the queued on_close.
        fake.deliverError(
          native,
          new Error("h2 protocol error: UnexpectedEof"),
        );
        fake.deliverClose(native);
        await vi.advanceTimersByTimeAsync(0);
        // Every pending createStream retry timer fires.
        await vi.advanceTimersByTimeAsync(RETRY_DELAY * 2);
      }

      fake.emitToActive(msg("same-message-id"));
      await vi.advanceTimersByTimeAsync(0);

      expect({
        activeNativeStreams: fake.active().length,
        delivered,
      }).toEqual({
        activeNativeStreams: 1,
        delivered: ["same-message-id"],
      });
    });
  });
});
