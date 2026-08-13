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
 * Creates a stream from a stream function
 *
 * If the stream fails, an attempt will be made to restart it.
 *
 * Ending the stream is terminal: no callbacks are invoked and no native
 * stream is created after the stream ends.
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
    onEnd,
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

  // lifecycle state, owned by this wrapper
  let stopped = false;
  // reading the flag through a function call defeats TS control-flow
  // narrowing, which cannot see the closure mutation across awaits
  const isStopped = () => stopped;
  let currentCloser: StreamCloser | undefined;
  let retryTimer: ReturnType<typeof setTimeout> | undefined;
  let retryInFlight = false;
  // the retry budget is monotonic: it is never reset for the lifetime of
  // this wrapper, so restarts are bounded even across successful restarts
  let remainingRetries = retryAttempts;

  // terminal transition: cancel any pending retry, close the active native
  // stream, and notify onEnd exactly once
  const stop = () => {
    if (isStopped()) {
      return;
    }
    stopped = true;
    if (retryTimer !== undefined) {
      clearTimeout(retryTimer);
      retryTimer = undefined;
    }
    currentCloser?.end();
    currentCloser = undefined;
    onEnd?.();
  };
  // registered before any async work so ending the stream is always terminal,
  // even while a retry is pending or a native stream is being created
  asyncStream.onDone = stop;

  const fail = (error: Error) => {
    if (isStopped()) {
      return;
    }
    // the terminal transition must run even if onError throws, otherwise a
    // throwing consumer callback leaves the stream open and hangs next()
    try {
      onError?.(error);
    } finally {
      void asyncStream.end();
    }
  };

  const streamCallback: StreamCallback<T> = (error, value) => {
    // an ended stream must not invoke any callbacks
    if (isStopped()) {
      return;
    }
    // if a stream error occurs, call the onError callback
    if (error) {
      onError?.(error);
      return;
    }
    // ensure the value is not undefined
    if (value !== undefined) {
      try {
        // if a streamValueMutator is provided, mutate the value
        if (streamValueMutator) {
          const mutatedValue = streamValueMutator(value);
          if (isPromise(mutatedValue)) {
            void mutatedValue
              .then((mutatedValue) => {
                // the stream may have ended while the value was mutating
                if (!isStopped() && mutatedValue !== undefined) {
                  asyncStream.push(mutatedValue);
                  onValue?.(mutatedValue);
                }
              })
              .catch((error: unknown) => {
                if (!isStopped()) {
                  onError?.(error as Error);
                }
              });
          } else {
            // a synchronous mutator may have ended the stream; gate delivery
            // on the stopped flag to match the async branch above
            if (!isStopped() && mutatedValue !== undefined) {
              asyncStream.push(mutatedValue);
              onValue?.(mutatedValue);
            }
          }
        } else {
          asyncStream.push(value as unknown as V);
          onValue?.(value as unknown as V);
        }
      } catch (error) {
        onError?.(error as Error);
      }
    }
  };

  const scheduleRetry = () => {
    // at most one retry may be in flight per wrapper
    if (isStopped() || retryInFlight) {
      return;
    }
    if (remainingRetries <= 0) {
      fail(new StreamFailedError(retryAttempts));
      return;
    }
    retryInFlight = true;
    retryTimer = setTimeout(() => {
      retryTimer = undefined;
      void attemptRestart();
    }, retryDelay);
  };

  const attemptRestart = async () => {
    if (isStopped()) {
      retryInFlight = false;
      return;
    }
    remainingRetries -= 1;
    onRetry?.(retryAttempts - remainingRetries, retryAttempts);
    try {
      // attempt to restart the stream
      const streamCloser = await streamFunction(
        streamCallback,
        handleNativeClose,
      );
      if (isStopped()) {
        // the stream ended while the native stream was being created
        streamCloser.end();
        retryInFlight = false;
        return;
      }
      await streamCloser.waitForReady();
      if (isStopped()) {
        streamCloser.end();
        retryInFlight = false;
        return;
      }
      currentCloser = streamCloser;
      retryInFlight = false;
      // stream restarted, call the onRestart callback
      onRestart?.();
    } catch (error) {
      retryInFlight = false;
      if (isStopped()) {
        return;
      }
      onError?.(error as Error);
      scheduleRetry();
    }
  };

  const handleNativeClose = () => {
    // ending the stream closes the native stream, which still triggers this
    // callback; only an unexpected close is a failure
    if (isStopped()) {
      return;
    }
    currentCloser = undefined;
    onFail?.();
    if (retryOnFail) {
      scheduleRetry();
    } else {
      fail(new StreamFailedError(0));
    }
  };

  try {
    // create the stream
    const streamCloser = await streamFunction(
      streamCallback,
      handleNativeClose,
    );
    if (isStopped()) {
      streamCloser.end();
    } else {
      await streamCloser.waitForReady();
      if (isStopped()) {
        streamCloser.end();
      } else {
        currentCloser = streamCloser;
      }
    }
  } catch (error) {
    onError?.(error as Error);
    if (retryOnFail) {
      scheduleRetry();
    } else {
      fail(new StreamFailedError(0));
    }
  }

  // return a proxy for the async stream
  return createAsyncStreamProxy(asyncStream);
};
