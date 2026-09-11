import type {
  ActionErrorData,
  ActionName,
  ActionWithoutData,
  EndStreamAction,
  ExtractActionData,
  ExtractActionResult,
  UnknownAction,
} from "@/types/actions";
import type {
  StreamAction,
  StreamActionErrorData,
} from "@/types/actions/streams";
import type { StreamOptions } from "@/utils/streams";
import { uuid } from "@/utils/uuid";

/**
 * Class that sets up a bridge for worker communications
 *
 * This class is not meant to be used directly.
 *
 * @param worker - The worker to use for communications
 * @param enableLogging - Whether to enable logging in the worker
 * @returns A new WorkerBridge instance
 */
export class WorkerBridge<T extends UnknownAction> {
  #worker: Worker;
  #enableLogging: boolean;
  #closed = false;
  #streamHandlers = new Set<
    (event: MessageEvent<StreamAction | StreamActionErrorData>) => void
  >();
  #promises = new Map<
    string,
    {
      resolve: (value: unknown) => void;
      reject: (reason?: unknown) => void;
    }
  >();

  constructor(worker: Worker, enableLogging?: boolean) {
    this.#worker = worker;
    this.#worker.addEventListener("message", this.handleMessage);
    this.#worker.addEventListener("error", this.handleError);
    this.#enableLogging = enableLogging ?? false;
  }

  /**
   * Sends an action message to the worker
   *
   * @param action - The action to send to the worker
   * @param data - The data to send to the worker
   * @returns A promise that resolves when the action is completed
   */
  action<
    A extends ActionName<T>,
    D = ExtractActionData<T, A>,
    R = ExtractActionResult<T, A>,
  >(action: A, ...args: D extends undefined ? [] : [data: D]) {
    if (this.#closed) throw new Error("The client is closed");
    const promiseId = uuid();
    this.#worker.postMessage({
      action,
      id: promiseId,
      data: args[0],
    });
    const promise = new Promise((resolve, reject) => {
      this.#promises.set(promiseId, {
        resolve: resolve,
        reject,
      });
    });
    return promise as [R] extends [undefined] ? Promise<void> : Promise<R>;
  }

  /**
   * Handles a message from the worker
   *
   * @param event - The event to handle
   */
  handleMessage = (
    event: MessageEvent<ActionWithoutData<T> | ActionErrorData<T>>,
  ) => {
    const eventData = event.data;
    if (this.#enableLogging) {
      console.log("[worker] client received event data", eventData);
    }
    const promise = this.#promises.get(eventData.id);
    if (promise) {
      this.#promises.delete(eventData.id);
      if ("error" in eventData) {
        promise.reject(eventData.error);
      } else {
        promise.resolve(eventData.result);
      }
    }
  };

  handleError = (event: ErrorEvent) => {
    console.error(`[worker] error: ${event.message}`);
    const error: unknown = event.error;
    this.close(error instanceof Error ? error : new Error(event.message));
  };

  /**
   * Handles a stream message from the worker
   *
   * @param streamId - The ID of the stream to handle
   * @param callback - The callback to handle the stream message
   * @returns A function to remove the stream handler
   */
  handleStreamMessage = <R extends StreamAction["result"], V = R>(
    streamId: string,
    callback: (error: Error | null, value: R | undefined) => void,
    options?: StreamOptions<R, V>,
  ) => {
    const streamHandler = (
      event: MessageEvent<StreamAction | StreamActionErrorData>,
    ) => {
      if (this.#closed) return;
      const eventData = event.data;
      // only handle messages for the passed stream ID
      if (eventData.streamId === streamId) {
        // if the stream failed, call the onFail callback
        if (eventData.action === "stream.fail") {
          options?.onFail?.();
          return;
        }
        if ("error" in eventData) {
          callback(eventData.error, undefined);
        } else {
          callback(null, eventData.result as R);
        }
      }
    };
    this.#worker.addEventListener("message", streamHandler);
    this.#streamHandlers.add(streamHandler);

    return async () => {
      this.#worker.removeEventListener("message", streamHandler);
      this.#streamHandlers.delete(streamHandler);
      if (!this.#closed)
        await this.action<
          "endStream",
          EndStreamAction["data"],
          EndStreamAction["result"]
        >("endStream", { streamId });
    };
  };

  /**
   * Removes all event listeners and terminates the worker
   */
  close(error = new Error("The client is closed")) {
    this.#closed = true;
    this.#detachStreams();
    for (const pending of this.#promises.values()) {
      pending.reject(error);
    }
    this.#promises.clear();
    this.#worker.removeEventListener("message", this.handleMessage);
    this.#worker.removeEventListener("error", this.handleError);
    this.#worker.terminate();
  }

  get isClosed() {
    return this.#closed;
  }

  /** Fence app dispatch now. Keep the worker until core close releases its database. */
  closeAfter(operation: Promise<unknown>): Promise<void> {
    this.#closed = true;
    this.#detachStreams();
    return operation
      .then(() => {})
      .finally(() => {
        this.close();
      });
  }

  #detachStreams() {
    for (const handler of this.#streamHandlers) {
      this.#worker.removeEventListener("message", handler);
    }
    this.#streamHandlers.clear();
  }
}
