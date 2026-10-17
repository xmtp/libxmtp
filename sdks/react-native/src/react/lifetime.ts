import { Client, XmtpError } from "@xmtp/react-native-sdk";

import type { InitializeClientOptions, XmtpContextValue } from "./types.js";

type Snapshot = Pick<XmtpContextValue, "client" | "error" | "isLoading">;
type Owner = {
  readonly client: Client;
  state: "live" | "retired" | "closing" | "failed";
  closing?: Promise<void>;
  error?: Error;
};
type Attempt = {
  readonly generation: number;
  readonly promise: Promise<Client>;
  readonly resolve: (client: Client) => void;
  readonly reject: (error: Error) => void;
};

export function asError(cause: unknown): Error {
  return cause instanceof Error
    ? cause
    : new Error("XMTP operation failed", { cause });
}

function cancelled(): Error {
  return new XmtpError.Cancelled({
    code: "Cancelled",
    category: "lifecycle",
    retryable: false,
    message: "XMTP initialization was cancelled",
  });
}

export function isCancelled(error: Error): boolean {
  return error instanceof XmtpError.Cancelled;
}

/** Callbacks cannot replace the SDK failure or expose its contents to a log. */
export function reportError(
  error: Error,
  callback?: (error: Error) => void,
): void {
  if (callback) {
    try {
      callback(error);
      return;
    } catch {
      // Keep the original command failure, including when an observer throws.
    }
  }
  console.error("XMTP React client cleanup or error observer failed");
}

/** One controller survives effect replay. Construction has no SDK side effects. */
export class ClientLifetime {
  private snapshot: Snapshot = { client: null, error: null, isLoading: false };
  private readonly listeners = new Set<() => void>();
  private readonly owners = new Map<Client, Owner>();
  private readonly ended = new WeakSet<Client>();
  private readonly constructors = new Set<Promise<void>>();
  private generation = 0;
  private active = false;
  private attempt?: Attempt;
  private replacement?: Promise<void>;
  private prop: Client | null | undefined;
  private hasProp = false;
  private onError?: (error: Error) => void;

  constructor(client?: Client | null) {
    this.prop = client;
    this.hasProp = true;
    this.snapshot = { client: client ?? null, error: null, isLoading: false };
  }

  readonly getSnapshot = (): Snapshot => this.snapshot;
  readonly subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  mount(): void {
    this.active = true;
  }

  setErrorHandler(callback?: (error: Error) => void): void {
    this.onError = callback;
  }

  unmount(): void {
    this.active = false;
    this.hasProp = false;
    this.invalidate();
    this.background(this.cleanup(false));
  }

  /** Only committed prop changes enter here; rendering never ends a client. */
  replace(client: Client | null | undefined): void {
    if (this.hasProp && this.prop === client) return;
    const replay = !this.hasProp && this.prop === client;
    this.hasProp = true;
    this.prop = client;
    // Feeding an owned client back as a prop does not transfer its ownership.
    if (client && this.snapshot.client === client) return;

    // Effect cleanup already fenced this generation during StrictMode replay.
    if (!replay) this.invalidate();
    const generation = this.generation;
    if (client && (this.owners.has(client) || this.ended.has(client))) {
      this.failure(
        new TypeError("A closing or closed XMTP client cannot be installed"),
      );
      this.background(this.cleanup(false));
      return;
    }
    const replacement = this.installBorrowed(client ?? null, generation);
    this.replacement = replacement;
    this.background(replacement);
  }

  readonly initialize = (options: InitializeClientOptions): Promise<Client> => {
    // Child layout effects run before the provider's committed prop effect.
    // Fence admission so a replaced client cannot escape through an old context.
    const generation = this.generation;
    return Promise.resolve().then(() => {
      if (!this.current(generation)) throw cancelled();
      return this.initializeCurrent(options);
    });
  };

  private initializeCurrent(options: InitializeClientOptions): Promise<Client> {
    if (this.snapshot.client) return Promise.resolve(this.snapshot.client);
    if (this.attempt) return this.attempt.promise;
    let resolve!: Attempt["resolve"];
    let reject!: Attempt["reject"];
    const promise = new Promise<Client>((accept, decline) => {
      resolve = accept;
      reject = decline;
    });
    const attempt = { generation: this.generation, promise, resolve, reject };
    this.attempt = attempt;
    this.publish({ ...this.snapshot, error: null, isLoading: true });
    const task = this.create(attempt, options);
    this.constructors.add(task);
    void task.then(() => this.constructors.delete(task));
    return promise;
  }

  readonly disconnect = async (): Promise<void> => {
    this.invalidate();
    try {
      // Pending wallet calls are fenced, but are not part of this await.
      await this.cleanup(true);
    } catch (cause) {
      const error = asError(cause);
      this.failure(error);
      throw error;
    }
  };

  private async installBorrowed(
    client: Client | null,
    generation: number,
  ): Promise<void> {
    do {
      await this.cleanup(false);
      if (!this.current(generation)) return;
      // A late constructor can settle between cleanup's return and this turn.
    } while (this.hasCleanup());
    this.publish({
      client,
      error: null,
      isLoading: this.attempt !== undefined,
    });
  }

  private async create(
    attempt: Attempt,
    options: InitializeClientOptions,
  ): Promise<void> {
    let acquired: Owner | undefined;
    try {
      await this.replacement;
      do {
        await this.cleanup(false);
        if (!this.current(attempt.generation)) return;
      } while (this.hasCleanup());
      if (this.snapshot.client) {
        const client = this.snapshot.client;
        this.attempt = undefined;
        this.publish({ client, error: null, isLoading: false });
        attempt.resolve(client);
        return;
      }
      const client = await Client.create(options.signer, options.options);
      acquired = { client, state: "live" };
      this.owners.set(client, acquired);
      if (!this.current(attempt.generation)) return;
      // Abandoned constructors may produce owned handles while this create
      // waits for its wallet. Their cleanup must succeed before publication.
      do {
        await this.cleanup(false);
        if (!this.current(attempt.generation)) return;
      } while (this.hasCleanup());
      this.attempt = undefined;
      this.publish({ client, error: null, isLoading: false });
      acquired = undefined;
      attempt.resolve(client);
    } catch (cause) {
      if (!this.current(attempt.generation)) return;
      const error = asError(cause);
      this.attempt = undefined;
      this.publish({
        client: null,
        error: isCancelled(error) ? null : error,
        isLoading: false,
      });
      attempt.reject(error);
    } finally {
      if (acquired) {
        if (acquired.state === "live") acquired.state = "retired";
        // A failed publication still owns its new handle. Keep the primary
        // failure visible if releasing this handle fails too.
        this.background(this.close(acquired), true);
      }
    }
  }

  private hasCleanup(): boolean {
    return [...this.owners.values()].some((owner) => owner.state !== "live");
  }

  private current(generation: number): boolean {
    return this.active && this.generation === generation;
  }

  private invalidate(): void {
    this.generation += 1;
    this.attempt?.reject(cancelled());
    this.attempt = undefined;
    this.replacement = undefined;
    // Include acquired clients still waiting for the publication barrier.
    for (const owner of this.owners.values()) {
      if (owner.state === "live") owner.state = "retired";
    }
    this.publish({ client: null, error: null, isLoading: false });
  }

  private close(owner: Owner, retry = false): Promise<void> {
    if (!this.owners.has(owner.client)) return Promise.resolve();
    if (owner.closing) return owner.closing;
    if (owner.state === "failed" && !retry) {
      return Promise.reject(
        owner.error ?? new Error("XMTP client cleanup failed"),
      );
    }
    owner.state = "closing";
    const closing = Promise.resolve()
      .then(() => owner.client.end())
      .then(
        () => {
          this.owners.delete(owner.client);
          this.ended.add(owner.client);
        },
        (cause: unknown) => {
          owner.state = "failed";
          owner.error = asError(cause);
          throw owner.error;
        },
      )
      .finally(() => {
        owner.closing = undefined;
      });
    owner.closing = closing;
    return closing;
  }

  private async cleanup(retry: boolean): Promise<void> {
    // Drain handles that become known while other handles are ending. Never
    // wait for an unresolved constructor, and retry each failed owner at most once.
    const attempted = new Set<Owner>();
    for (;;) {
      const owners = [...this.owners.values()].filter(
        (owner) => owner.state !== "live" && !attempted.has(owner),
      );
      if (!owners.length) return;
      const results = await Promise.allSettled(
        owners.map((owner) => {
          attempted.add(owner);
          return this.close(owner, retry);
        }),
      );
      const failed = results.find((result) => result.status === "rejected");
      if (failed?.status === "rejected") throw asError(failed.reason);
    }
  }

  private background(task: Promise<void>, preserveError = false): void {
    void task.catch((cause: unknown) => {
      const error = asError(cause);
      if (!preserveError || !this.snapshot.error) this.failure(error);
      reportError(error, this.onError);
    });
  }

  private failure(error: Error): void {
    if (this.active && !isCancelled(error))
      this.publish({ ...this.snapshot, error });
  }

  private publish(snapshot: Snapshot): void {
    this.snapshot = snapshot;
    if (this.active) this.listeners.forEach((listener) => listener());
  }
}
