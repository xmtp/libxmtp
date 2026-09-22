import { isPromise } from "node:util/types";

import type { StreamCloser } from "@xmtp/node-bindings";

import { AsyncStream, createAsyncStreamProxy } from "@/AsyncStream";

import { StreamFailedError, StreamInvalidRetryAttemptsError } from "./errors";
import { getStreamFailureDetails } from "./streamFailure";

export const DEFAULT_RETRY_DELAY = 60_000; // milliseconds
export const DEFAULT_RETRY_ATTEMPTS = 10;

// Core reports storage failures and network exhaustion as terminal.
// Reopening here would bypass that boundary and hide the original failure.
const isTerminalNativeFailure = (error: unknown) => {
  if (
    error instanceof Error &&
    /^\[(?:LocalDeliveryError::(?:NetworkRecoveryExhausted|NetworkFailure)|ClientError::(?:BackendMismatch|ClientVersionTooOld)|(?:SubscribeError|GroupError|ClientError)::(?:Db|Storage)|GroupError::SqlKeyStore)\]/.test(
      error.message,
    )
  )
    return true;
  return (
    getStreamFailureDetails(error)?.barriers.some((barrier) =>
      barrier.unfinished.some(
        ({ cause }) =>
          cause?.kind === "storage" ||
          (cause?.kind === "receiver" && cause.code === "incoming_storage"),
      ),
    ) ?? false
  );
};

/**
 * Notification streams created by createStream use the retry settings and hooks.
 * Durable message streams use onValue, onError, and onEnd. Core handles their
 * network recovery; notification retry settings and hooks do not apply.
 */
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
   * Called when a notification stream fails
   */
  onFail?: () => void;
  /**
   * Called when a notification stream is restarted
   */
  onRestart?: () => void;
  /**
   * Called when a notification stream is retried
   */
  onRetry?: (attempts: number, maxAttempts: number) => void;
  /**
   * Called when a value is emitted from the stream.
   * For message streams, this selects callback mode. Do not also iterate that stream.
   * Message delivery is acknowledged after this callback returns successfully.
   */
  onValue?: (value: V) => void | Promise<void>;
  /**
   * The number of times to retry an unexpected notification stream closure.
   * Terminal native storage errors are not retried.
   * (default: 10)
   */
  retryAttempts?: number;
  /**
   * The delay between notification stream retries (in milliseconds)
   * (default: 60000)
   */
  retryDelay?: number;
  /**
   * Whether to retry a notification stream if it fails
   * (default: true)
   */
  retryOnFail?: boolean;
  /**
   * Whether to skip pre-sync for notification streams.
   * Durable message readers start receipt without a separate pre-sync.
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
 * Unexpected native closures can be retried. Terminal native storage errors end the stream and reject iteration with the original cause.
 *
 * Ending the stream is terminal: no callbacks are invoked and no native
 * stream is created after the stream ends.
 *
 * This function is not intended to be used directly.
 *
 * @param streamFunction - The stream function to create a stream from
 * @param streamValueMutator - An optional function to mutate the value emitted from the stream
 * @param options - The options for the stream
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
  // set when a restart's native stream closes during its own creation, so the
  // completed attempt reschedules instead of installing an already-dead closer
  let closePendingDuringRestart = false;
  let initialClosePending = false;
  // read through a call so no-unnecessary-condition cannot narrow the flag to
  // a constant; handleNativeClose mutates it across an await
  const isClosePending = () => closePendingDuringRestart;
  // This finite budget only restarts a native loop that closes without a terminal cause.
  let remainingRetries = retryAttempts;
  let generation = 0;
  let terminalError: Error | undefined;
  const pendingReads = new Set<{ error?: Error }>();
  let lastError: Error | undefined;
  const next = asyncStream.next;
  asyncStream.next = async () => {
    const read = { error: terminalError };
    terminalError = undefined;
    pendingReads.add(read);
    const throwTerminalError = () => {
      if (read.error) throw read.error;
    };
    try {
      throwTerminalError();
      const value = await next();
      throwTerminalError();
      // A value resolved before failure can still be queued on the JS task.
      return isStopped() ? { done: true, value: undefined } : value;
    } finally {
      pendingReads.delete(read);
    }
  };

  const reportError = (error: Error) => {
    try {
      void Promise.resolve(onError?.(error)).catch(() => undefined);
    } catch {
      // Error handlers must not throw from the native callback task.
    }
  };

  // terminal transition: cancel any pending retry, close the active native
  // stream, and notify onEnd exactly once
  const stop = () => {
    if (isStopped()) {
      return;
    }
    stopped = true;
    generation += 1;
    closePendingDuringRestart = false;
    if (retryTimer !== undefined) {
      clearTimeout(retryTimer);
      retryTimer = undefined;
    }
    const closer = currentCloser;
    currentCloser = undefined;
    try {
      closer?.end();
    } catch {
      // The stopped generation stays fenced if native cleanup fails.
    }
    try {
      void Promise.resolve(onEnd?.()).catch(() => undefined);
    } catch {
      // A synchronous end handler must not interrupt iterator completion.
    }
  };
  // registered before any async work so ending the stream is always terminal,
  // even while a retry is pending or a native stream is being created
  asyncStream.onDone = stop;

  const fail = (error: Error) => {
    if (isStopped()) {
      return;
    }
    // Reserve the cause for the oldest pending read. Reentrant next() calls
    // from onError must not consume another call's rejection.
    const read = pendingReads.values().next().value;
    if (read) read.error = error;
    else terminalError = error;
    // Fence callbacks and close before application code opens a replacement.
    // Preserve the original cause if an end handler fails.
    try {
      void asyncStream.end().catch(() => undefined);
    } catch {
      // stop() has already fenced this generation.
    }
    reportError(error);
  };

  const handleAsyncError = (error: unknown) => {
    if (!isStopped()) {
      if (isTerminalNativeFailure(error)) fail(error as Error);
      else reportError(error as Error);
    }
  };

  const streamCallback = (
    error: Error | null,
    value: T | undefined,
    isCurrent: () => boolean,
  ) => {
    // An ended or replaced native stream cannot invoke callbacks.
    if (!isCurrent()) {
      return;
    }
    // if a stream error occurs, call the onError callback
    if (error) {
      lastError = error;
      if (isTerminalNativeFailure(error)) fail(error);
      else reportError(error);
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
                  return onValue?.(mutatedValue);
                }
              })
              .catch(handleAsyncError);
          } else {
            // a synchronous mutator may have ended the stream; gate delivery
            // on the stopped flag to match the async branch above
            if (!isStopped() && mutatedValue !== undefined) {
              asyncStream.push(mutatedValue);
              void Promise.resolve(onValue?.(mutatedValue)).catch(
                handleAsyncError,
              );
            }
          }
        } else {
          asyncStream.push(value as unknown as V);
          void Promise.resolve(onValue?.(value as unknown as V)).catch(
            handleAsyncError,
          );
        }
      } catch (error) {
        handleAsyncError(error);
      }
    }
  };

  const scheduleRetry = () => {
    // at most one retry may be in flight per wrapper
    if (isStopped() || retryInFlight) {
      return;
    }
    if (remainingRetries <= 0) {
      const error = new StreamFailedError(retryAttempts);
      error.cause = lastError;
      fail(error);
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
    // scope the pending-close flag to this attempt: a close recorded while the
    // retry timer was merely pending belongs to the stream that scheduled it
    closePendingDuringRestart = false;
    remainingRetries -= 1;
    onRetry?.(retryAttempts - remainingRetries, retryAttempts);
    if (isStopped()) {
      // onRetry may have ended the stream; do not open a native stream after
      // termination
      retryInFlight = false;
      return;
    }
    try {
      // attempt to restart the stream
      const streamCloser = await openNative();
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
      if (isClosePending()) {
        // the replacement stream closed during its own creation; discard it
        // and schedule a fresh attempt instead of installing a dead closer
        closePendingDuringRestart = false;
        streamCloser.end();
        retryInFlight = false;
        scheduleRetry();
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
      closePendingDuringRestart = false;
      lastError = error as Error;
      if (isTerminalNativeFailure(lastError)) fail(lastError);
      else {
        reportError(lastError);
        scheduleRetry();
      }
    }
  };

  const handleNativeClose = () => {
    // ending the stream closes the native stream, which still triggers this
    // callback; only an unexpected close is a failure
    if (isStopped()) {
      return;
    }
    if (generation === 1) initialClosePending = true;
    currentCloser = undefined;
    onFail?.();
    if (retryOnFail) {
      // a native close during an in-flight restart is dropped by the
      // single-flight guard; record it so the completed attempt reschedules
      // instead of installing a stream that already died
      if (retryInFlight) {
        closePendingDuringRestart = true;
        return;
      }
      scheduleRetry();
    } else {
      fail(new StreamFailedError(0));
    }
  };

  const openNative = () => {
    const openedGeneration = ++generation;
    const isCurrent = () => !isStopped() && openedGeneration === generation;
    return streamFunction(
      (error, value) => {
        streamCallback(error, value, isCurrent);
      },
      () => {
        if (openedGeneration === generation) handleNativeClose();
      },
    );
  };

  const initialGeneration = generation + 1;
  const initialWasClosed = () =>
    initialClosePending || generation !== initialGeneration;
  try {
    // create the stream
    const streamCloser = await openNative();
    if (isStopped() || initialWasClosed()) {
      streamCloser.end();
    } else {
      await streamCloser.waitForReady();
      if (isStopped() || initialWasClosed()) {
        streamCloser.end();
      } else {
        currentCloser = streamCloser;
      }
    }
  } catch (error) {
    if (initialWasClosed()) return createAsyncStreamProxy(asyncStream);
    lastError = error as Error;
    if (isTerminalNativeFailure(lastError)) fail(lastError);
    else if (retryOnFail) {
      reportError(lastError);
      scheduleRetry();
    } else {
      fail(lastError);
    }
  }

  // return a proxy for the async stream
  return createAsyncStreamProxy(asyncStream);
};
