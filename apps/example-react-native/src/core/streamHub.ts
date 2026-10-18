// The session's stream readers (ExampleSDK/ConversationQueries.swift
// `readers`, App/AppGraph.swift `startReaders`, and open PR #4586).
//
// - Exactly one default message reader per session. It owns the database's
//   consumer lease, so it runs only while the app is in the foreground;
//   closing it releases the lease and delivery resumes from the saved
//   position. The conversation and event readers hold no lease and keep
//   running.
// - Every listener finishes before the next read, because the next read
//   acknowledges the previous message.
// - A reader that fails shows "Disconnected. Retrying…" and restarts after
//   one second.

import type {
  ClientEvent,
  ConnectionState,
  EventKind,
} from "@xmtp/react-native-sdk";

import { bytesToHex } from "./hex.ts";
import { Store } from "./store.ts";

/** The event kinds the session reads (`sessionEventKinds` in iOS). */
export const SESSION_EVENT_KINDS: readonly EventKind[] = [
  "handle.changed",
  "consent.changed",
  "message.deleted",
];

export type SessionEvent =
  | {
      readonly kind: "handleChanged";
      readonly inboxId: string;
      readonly handle: string;
    }
  | {
      readonly kind: "messageDeleted";
      readonly conversationId: string;
      readonly messageId: string;
      readonly locallyDeleted: boolean;
    }
  /** Consent changed, or the event queue overflowed: reload what you show. */
  | { readonly kind: "changed"; readonly cause: "consent" | "lagged" };

/** The session event for an SDK event, or undefined for kinds the app ignores. */
export function sessionEvent(event: ClientEvent): SessionEvent | undefined {
  switch (event.kind) {
    case "handle.changed":
      return {
        kind: "handleChanged",
        inboxId: event.handle_changed.inbox_id,
        handle: event.handle_changed.handle,
      };
    case "message.deleted":
      return {
        kind: "messageDeleted",
        conversationId: bytesToHex(event.message_deleted.group_id),
        messageId: bytesToHex(event.message_deleted.message_id),
        locallyDeleted: event.message_deleted.cause === "deleted_locally",
      };
    case "consent.changed":
      return { kind: "changed", cause: "consent" };
    case "lagged":
      return { kind: "changed", cause: "lagged" };
    default:
      return undefined;
  }
}

export const DISCONNECTED_RETRYING = "Disconnected. Retrying…";

/** The Chats banner for the message stream's connection state (none when connected). */
export function connectionBanner(
  state: ConnectionState | undefined,
): string | undefined {
  switch (state) {
    case "connecting":
      return "Connecting to your server…";
    case "reconnecting":
      return "Reconnecting to your server…";
    case "failed":
      return "Cannot connect to your server.";
    case "closed":
      return "Not connected to your server.";
    case "connected":
    case undefined:
      return undefined;
  }
}

/** One open SDK reader. `next` after the last value acknowledges it. */
export interface StreamReader<T> {
  next(): Promise<IteratorResult<T>>;
  end(): Promise<void>;
  /** Resolves once the reader subscribed; rejects if it could not open. */
  ready?(): Promise<void>;
}

/** Opens the three readers of a client. */
export interface StreamSource<M, C> {
  messages(onState: (state: ConnectionState) => void): StreamReader<M>;
  conversations(): StreamReader<C>;
  events(kinds: readonly EventKind[]): Promise<StreamReader<ClientEvent>>;
}

export type StreamListener<T> = (value: T) => void | Promise<void>;

export interface ConnectionStatus {
  /** The message stream's transport state; undefined before its first report. */
  readonly state: ConnectionState | undefined;
  /** The Chats banner text, or undefined for none. */
  readonly banner: string | undefined;
}

export interface StreamHubOptions {
  /** Whether the app is in the foreground now (the message reader runs only then). */
  readonly foreground: boolean;
  /** False once the session ended or was replaced. */
  readonly isCurrent: () => boolean;
  readonly retryDelayMs?: number;
  /**
   * A restarted reader clears the retry banner once it delivers a value or
   * stays open this long (default 2 s). A reader that reports `connected` and
   * then fails at once (for example with `ConsumerOwned`) keeps the banner.
   */
  readonly healthyAfterMs?: number;
  /** Waits `ms` or until `signal` aborts (tests pass a fast fake). */
  readonly sleep?: (ms: number, signal: AbortSignal) => Promise<void>;
}

class ConnectionStore extends Store<ConnectionStatus> {
  update(status: ConnectionStatus): void {
    const current = this.getState();
    if (current.state === status.state && current.banner === status.banner) {
      return;
    }
    this.setState(status);
  }
}

function defaultSleep(ms: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve) => {
    if (isAborted(signal)) {
      resolve();
      return;
    }
    const timer = setTimeout(() => {
      signal.removeEventListener("abort", done);
      resolve();
    }, ms);
    const done = (): void => {
      clearTimeout(timer);
      resolve();
    };
    signal.addEventListener("abort", done, { once: true });
  });
}

/** Reads the signal each time (TypeScript narrows `signal.aborted` across awaits). */
function isAborted(signal: AbortSignal): boolean {
  return signal.aborted;
}

async function endQuietly(reader: StreamReader<unknown>): Promise<void> {
  try {
    await reader.end();
  } catch {
    // A reader of an ended client may fail to end; it is closed either way.
  }
}

interface LaneOptions<T> {
  readonly name: string;
  /** Opens a reader. `run` identifies this attempt for state callbacks. */
  readonly open: (run: symbol) => StreamReader<T> | Promise<StreamReader<T>>;
  readonly deliver: (value: T) => Promise<void>;
  /** The reader delivered a value or stayed open for `healthyAfterMs`. */
  readonly healthy: (run: symbol) => void;
  readonly failed: (error: unknown) => void;
}

/** One reader that restarts after failures until it is stopped. */
class Lane<T> {
  readonly #options: LaneOptions<T>;
  readonly #hub: StreamHubOptions;
  #controller: AbortController | undefined;
  #running: Promise<void> | undefined;
  #reader: StreamReader<T> | undefined;
  #run: symbol | undefined;

  constructor(options: LaneOptions<T>, hub: StreamHubOptions) {
    this.#options = options;
    this.#hub = hub;
  }

  get running(): boolean {
    return this.#running !== undefined;
  }

  /** True when `run` is the attempt that is reading now. */
  isRun(run: symbol): boolean {
    return this.#run === run && this.#controller?.signal.aborted === false;
  }

  start(): void {
    if (this.#running !== undefined) return;
    const controller = new AbortController();
    this.#controller = controller;
    const running = this.#loop(controller.signal).finally(() => {
      if (this.#running === running) this.#running = undefined;
    });
    this.#running = running;
  }

  /** Ends the reader (releasing what it holds) and waits for the loop to stop. */
  async stop(): Promise<void> {
    this.#controller?.abort();
    const reader = this.#reader;
    if (reader !== undefined) await endQuietly(reader);
    await this.#running;
  }

  async #loop(signal: AbortSignal): Promise<void> {
    const sleep = this.#hub.sleep ?? defaultSleep;
    while (!isAborted(signal) && this.#hub.isCurrent()) {
      const failure = await this.#attempt(signal);
      if (
        failure === undefined ||
        isAborted(signal) ||
        !this.#hub.isCurrent()
      ) {
        return;
      }
      this.#options.failed(failure.error);
      await sleep(this.#hub.retryDelayMs ?? 1000, signal);
    }
  }

  /** One reader run: the failure, or undefined when the reader ended or stopped. */
  async #attempt(
    signal: AbortSignal,
  ): Promise<{ readonly error: unknown } | undefined> {
    const run = Symbol(this.#options.name);
    this.#run = run;
    let reader: StreamReader<T> | undefined;
    let healthy = false;
    let healthyTimer: ReturnType<typeof setTimeout> | undefined;
    const markHealthy = (): void => {
      if (healthy || this.#run !== run) return;
      healthy = true;
      this.#options.healthy(run);
    };
    try {
      reader = await this.#options.open(run);
      this.#reader = reader;
      if (isAborted(signal)) return undefined;
      await reader.ready?.();
      if (isAborted(signal)) return undefined;
      healthyTimer = setTimeout(markHealthy, this.#hub.healthyAfterMs ?? 2000);
      for (;;) {
        const item = await reader.next();
        if (item.done || isAborted(signal)) break;
        markHealthy();
        await this.#options.deliver(item.value);
      }
      // A reader that ends without an error is not restarted (iOS).
      return undefined;
    } catch (error) {
      return { error };
    } finally {
      clearTimeout(healthyTimer);
      if (this.#run === run) this.#run = undefined;
      if (this.#reader === reader) this.#reader = undefined;
      if (reader !== undefined) await endQuietly(reader);
    }
  }
}

/**
 * Fans the session's three readers out to listeners and reports the
 * connection banner. The session starts it once and closes it before it ends
 * the client.
 */
export class StreamHub<M, C> {
  /** The message stream's connection state and the Chats banner. */
  readonly connection = new ConnectionStore({
    state: undefined,
    banner: undefined,
  });
  readonly #options: StreamHubOptions;
  readonly #messageListeners = new Set<StreamListener<M>>();
  readonly #conversationListeners = new Set<StreamListener<C>>();
  readonly #eventListeners = new Set<StreamListener<SessionEvent>>();
  readonly #retrying = new Set<string>();
  readonly #messages: Lane<M>;
  readonly #conversations: Lane<C>;
  readonly #events: Lane<ClientEvent>;
  #messageState: ConnectionState | undefined;
  #foreground: boolean;
  #started = false;
  #closed = false;
  #transition: Promise<void> = Promise.resolve();

  constructor(source: StreamSource<M, C>, options: StreamHubOptions) {
    this.#options = options;
    this.#foreground = options.foreground;
    this.#messages = new Lane<M>(
      {
        name: "messages",
        open: (run) =>
          source.messages((state) => {
            this.#messageStateChanged(run, state);
          }),
        deliver: (message) => this.#deliver(this.#messageListeners, message),
        healthy: () => {
          this.#recovered("messages");
        },
        failed: (error) => {
          this.#failed("messages", error);
        },
      },
      options,
    );
    this.#conversations = new Lane<C>(
      {
        name: "conversations",
        open: () => source.conversations(),
        deliver: (conversation) =>
          this.#deliver(this.#conversationListeners, conversation),
        healthy: () => {
          this.#recovered("conversations");
        },
        failed: (error) => {
          this.#failed("conversations", error);
        },
      },
      options,
    );
    this.#events = new Lane<ClientEvent>(
      {
        name: "events",
        open: () => source.events(SESSION_EVENT_KINDS),
        deliver: async (event) => {
          const mapped = sessionEvent(event);
          if (mapped !== undefined) {
            await this.#deliver(this.#eventListeners, mapped);
          }
        },
        healthy: () => {
          this.#recovered("events");
        },
        failed: (error) => {
          this.#failed("events", error);
        },
      },
      options,
    );
  }

  /** True while the default message reader runs. */
  get readingMessages(): boolean {
    return this.#messages.running;
  }

  /** Starts the readers (the message reader only in the foreground). */
  start(): void {
    if (this.#started || this.#closed) return;
    this.#started = true;
    this.#conversations.start();
    this.#events.start();
    if (this.#foreground) this.#messages.start();
  }

  /**
   * Foreground: start the message reader. Background or inactive: close it,
   * which releases its consumer lease. Rapid changes apply the latest value.
   */
  setForeground(active: boolean): Promise<void> {
    this.#foreground = active;
    this.#transition = this.#transition.then(async () => {
      if (this.#closed || !this.#started) return;
      if (this.#foreground) {
        if (this.#options.isCurrent()) this.#messages.start();
      } else {
        await this.#messages.stop();
      }
    });
    return this.#transition;
  }

  /** Ends every reader and waits until no listener runs. */
  async close(): Promise<void> {
    this.#closed = true;
    await Promise.all([
      this.#messages.stop(),
      this.#conversations.stop(),
      this.#events.stop(),
    ]);
    await this.#transition;
  }

  /** Each message the default reader delivers. The next read waits for every listener. */
  onMessage(listener: StreamListener<M>): () => void {
    return this.#listen(this.#messageListeners, listener);
  }

  /** Each conversation the client joins or creates. */
  onConversation(listener: StreamListener<C>): () => void {
    return this.#listen(this.#conversationListeners, listener);
  }

  /** Handle changes, deleted messages, consent changes and lag. */
  onEvent(listener: StreamListener<SessionEvent>): () => void {
    return this.#listen(this.#eventListeners, listener);
  }

  #listen<T>(
    set: Set<StreamListener<T>>,
    listener: StreamListener<T>,
  ): () => void {
    // A wrapper keeps one registration per call, even for the same function.
    const entry: StreamListener<T> = (value) => listener(value);
    set.add(entry);
    return () => {
      set.delete(entry);
    };
  }

  async #deliver<T>(set: Set<StreamListener<T>>, value: T): Promise<void> {
    for (const listener of Array.from(set)) {
      try {
        await listener(value);
      } catch (error) {
        console.warn("Stream listener failed", error);
      }
    }
  }

  #messageStateChanged(run: symbol, state: ConnectionState): void {
    // A reader that was paused or replaced reports nothing more.
    if (
      this.#closed ||
      !this.#messages.isRun(run) ||
      !this.#options.isCurrent()
    ) {
      return;
    }
    this.#messageState = state;
    this.#publish();
  }

  #failed(lane: string, error: unknown): void {
    console.warn(`The ${lane} reader failed; retrying`, error);
    this.#retrying.add(lane);
    if (lane === "messages") this.#messageState = undefined;
    this.#publish();
  }

  #recovered(lane: string): void {
    if (this.#retrying.delete(lane)) this.#publish();
  }

  #publish(): void {
    this.connection.update({
      state: this.#messageState,
      banner:
        this.#retrying.size > 0
          ? DISCONNECTED_RETRYING
          : connectionBanner(this.#messageState),
    });
  }
}
