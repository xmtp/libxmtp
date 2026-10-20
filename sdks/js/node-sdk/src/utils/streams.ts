import { isPromise } from "node:util/types";
import type { StreamCloser } from "@xmtp/node-bindings";
import { AsyncStream, createAsyncStreamProxy } from "@/AsyncStream";
import { StreamFailedError, StreamInvalidRetryAttemptsError } from "./errors";

export const DEFAULT_RETRY_DELAY = 60_000; // milliseconds
export const DEFAULT_RETRY_ATTEMPTS = 10;

export type StreamOptions<T = unknown, V = T> = {
  /**
   * Called when the stream ends
   */
  onEnd?: () => void;
  /**
   * Called when a stream error occurs
   */
  onError?: (error: Error) => void;
  /**
   * Called when the stream fails
   */
  onFail?: () => void;
  /**
   * Called when the stream is restarted
   */
  onRestart?: () => void;
  /**
   * Called when the stream is retried
   */
  onRetry?: (attempts: number, maxAttempts: number) => void;
  /**
   * Called when a value is emitted from the stream
   */
  onValue?: (value: V) => void;
  /**
   * The number of times to retry the stream
   * (default: 10)
   */
  retryAttempts?: number;
  /**
   * The delay between retries (in milliseconds)
   * (default: 60000)
   */
  retryDelay?: number;
  /**
   * Whether to retry the stream if it fails
   * (default: true)
   */
  retryOnFail?: boolean;
  /**
   * Whether to disable network sync before starting the stream
   * (default: false)
   */
  disableSync?: boolean;
};

export type StreamCallback<T = unknown> = (
  error: Error | null,
  value: T | undefined,
) => void;

export type StreamFunction<T = unknown> = (
  callback: StreamCallback<T>,
  onFail: () => void,
) => Promise<StreamCloser>;

export type StreamValueMutator<T = unknown, V = T> = (
  value: T,
) => V | Promise<V>;

/**
 * One native stream that a wrapper opened.
 *
 * `closer` is set when `streamFunction` resolves. `ready` is set when the
 * stream reported ready and became the wrapper's live stream. `closed` is set
 * when the native `on_close` for this stream reached JS.
 */
type NativeStream = {
  closer?: StreamCloser;
  ready: boolean;
  closed: boolean;
};

/**
 * Creates a stream from a stream function
 *
 * If the stream fails, an attempt will be made to restart it.
 *
 * Lifecycle:
 * - `end()` and `return()` are terminal. They end the native stream that the
 *   wrapper owns, including one that is still starting, and cancel a pending
 *   retry. After that, no native stream is opened, and `onValue`, `onError`,
 *   `onFail`, `onRetry` and `onRestart` are not called.
 * - Only an unexpected native close calls `onFail` and starts a retry.
 *   `StreamCloser.end()` does not call the native `on_close`, and a close
 *   that reaches JS after `end()` or comes from an older native stream is
 *   ignored.
 * - At most one retry sequence runs at a time. `retryAttempts` bounds the
 *   number of consecutive failed restart attempts. A restart whose native
 *   stream reports ready and is still open is a success, so the next failure
 *   gets the full budget again.
 *
 * This function is not intended to be used directly.
 *
 * @param streamFunction - The stream function to create a stream from
 * @param streamValueMutator - An optional function to mutate the value emitted from the stream
 * @param options - The options for the stream
 * @param args - Additional arguments to pass to the stream function
 * @returns An async iterable stream proxy
 * @throws {StreamInvalidRetryAttemptsError} if the retryAttempts option is less than 0 and retryOnFail is true
 * @throws {StreamFailedError} if the stream fails and can't be restarted
 */
export const createStream = async <T = unknown, V = T>(
  streamFunction: StreamFunction<T>,
  streamValueMutator?: StreamValueMutator<T, V | undefined>,
  options?: StreamOptions<T, V>,
) => {
  const {
    onError,
    onFail,
    onRestart,
    onRetry,
    onValue,
    retryAttempts = DEFAULT_RETRY_ATTEMPTS,
    retryDelay = DEFAULT_RETRY_DELAY,
    retryOnFail = true,
  } = options ?? {};
  // retry attempts must be greater than 0
  if (retryOnFail && retryAttempts < 0) {
    throw new StreamInvalidRetryAttemptsError();
  }

  const asyncStream = new AsyncStream<V>();
  // Terminal. Set by end(), return(), or a final failure.
  let stopped = false;
  // The one native stream this wrapper owns. A close from any other native
  // stream is stale.
  let current: NativeStream | undefined;
  // True while a retry sequence runs. Keeps retries single-flight.
  let recovering = false;
  // Cancels the pending retry delay.
  let cancelDelay: (() => void) | undefined;

  // Read through functions: other callbacks change this state while an
  // async function awaits.
  const isStopped = () => stopped;
  const isCurrent = (native: NativeStream) => !stopped && current === native;

  // Installed before any await, so that end() always reaches the current
  // native stream, even one that is still starting.
  asyncStream.onDone = () => {
    if (stopped) {
      return;
    }
    stopped = true;
    cancelDelay?.();
    const native = current;
    current = undefined;
    native?.closer?.end();
  };

  const delay = (ms: number) =>
    new Promise<void>((resolve) => {
      const timer = setTimeout(() => {
        cancelDelay = undefined;
        resolve();
      }, ms);
      cancelDelay = () => {
        clearTimeout(timer);
        cancelDelay = undefined;
        resolve();
      };
    });

  const reportError = (error: unknown) => {
    if (!stopped) {
      onError?.(error as Error);
    }
  };

  const deliver = (value: V | undefined) => {
    if (stopped || value === undefined) {
      return;
    }
    asyncStream.push(value);
    onValue?.(value);
  };

  const streamCallback: StreamCallback<T> = (error, value) => {
    // a native callback queued before end() can reach JS after it
    if (stopped) {
      return;
    }
    // if a stream error occurs, call the onError callback
    if (error) {
      onError?.(error);
      return;
    }
    // ensure the value is not undefined
    if (value === undefined) {
      return;
    }
    try {
      // if a streamValueMutator is provided, mutate the value
      if (streamValueMutator) {
        const mutatedValue = streamValueMutator(value);
        if (isPromise(mutatedValue)) {
          // deliver() drops the value if the stream ended meanwhile
          void mutatedValue.then(deliver).catch(reportError);
        } else {
          deliver(mutatedValue);
        }
      } else {
        deliver(value as unknown as V);
      }
    } catch (error) {
      reportError(error);
    }
  };

  // Ends the stream and reports why. This error is the only callback that
  // runs after the stream ends.
  const fail = (error: StreamFailedError) => {
    void asyncStream.end();
    onError?.(error);
  };

  /**
   * Opens a native stream and makes it the current one.
   *
   * Resolves to true when the native stream is ready and open. Resolves to
   * false when the wrapper ended meanwhile, or the native stream closed
   * before it was ready; either way the new closer is ended, never kept.
   * Rejects when the stream function or waitForReady rejects.
   */
  const open = async () => {
    // a callback such as onRetry may have ended the stream
    if (stopped) {
      return false;
    }
    const native: NativeStream = { ready: false, closed: false };
    current = native;
    const closer = await streamFunction(streamCallback, () => {
      onNativeClose(native);
    });
    native.closer = closer;
    if (!isCurrent(native)) {
      closer.end();
      return false;
    }
    await closer.waitForReady();
    if (!isCurrent(native)) {
      closer.end();
      return false;
    }
    // A native stream that fails to start calls on_close and then reports
    // ready. That is a failed attempt, not a restart.
    if (native.closed || closer.isClosed()) {
      closer.end();
      return false;
    }
    native.ready = true;
    return true;
  };

  // Retries until a native stream is ready, the budget runs out, or the
  // stream ends. Each failed attempt spends one unit of the budget.
  const restart = async () => {
    try {
      for (let attempt = 1; attempt <= retryAttempts; attempt++) {
        // wait for the retry delay before attempting to restart the stream
        await delay(retryDelay);
        if (isStopped()) {
          return;
        }
        // call the onRetry callback
        onRetry?.(attempt, retryAttempts);
        let restarted = false;
        try {
          restarted = await open();
        } catch (error) {
          reportError(error);
        }
        if (isStopped()) {
          return;
        }
        if (restarted) {
          // stream restarted, call the onRestart callback
          try {
            onRestart?.();
          } catch (error) {
            reportError(error);
          }
          return;
        }
      }
      // the stream was not restarted within the budget
      fail(new StreamFailedError(retryAttempts));
    } finally {
      recovering = false;
    }
  };

  // Starts one retry sequence, or ends the stream if retries are off.
  const recover = () => {
    if (stopped || recovering) {
      return;
    }
    if (!retryOnFail) {
      // stream failed and should not be retried
      fail(new StreamFailedError(0));
      return;
    }
    recovering = true;
    void restart();
  };

  // The native on_close. It runs only when a native stream ends by itself,
  // for example after a transport error or a failed start.
  function onNativeClose(native: NativeStream) {
    if (!isCurrent(native) || native.closed) {
      return;
    }
    native.closed = true;
    // call the onFail callback
    onFail?.();
    // a native stream that is still starting is handled by its open() call
    if (native.ready) {
      recover();
    }
  }

  let started = false;
  try {
    // create the stream
    started = await open();
  } catch (error) {
    reportError(error);
  }
  if (!started) {
    recover();
  }

  // return a proxy for the async stream
  return createAsyncStreamProxy(asyncStream);
};
