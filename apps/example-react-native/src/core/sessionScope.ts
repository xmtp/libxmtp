// Session-owned work. A session cancels and awaits all of it before it ends
// its SDK client.

/** Work that runs until its signal aborts. */
export type ScopedTask = (signal: AbortSignal) => Promise<void>;
/** Teardown that runs when the session closes. */
export type ScopedCleanup = () => void | Promise<void>;

function report(error: unknown): void {
  console.warn("Session cleanup failed", error);
}

export class SessionScope {
  readonly #controller = new AbortController();
  readonly #tasks = new Set<Promise<void>>();
  #cleanups: ScopedCleanup[] = [];
  #finalCleanups: ScopedCleanup[] = [];
  #closed = false;

  /** Aborts when the session closes. */
  get signal(): AbortSignal {
    return this.#controller.signal;
  }

  /** True once `cancelAndWait` started. */
  get closed(): boolean {
    return this.#closed;
  }

  /**
   * The number of unfinished tasks. Only a test reads it: a finished task
   * that stayed in the set would make `cancelAndWait` loop on microtasks
   * forever, which no test timeout can catch.
   */
  get taskCount(): number {
    return this.#tasks.size;
  }

  /**
   * Runs `task` as tracked session work. After close it starts with an
   * aborted signal. The returned promise settles when the task does; a task
   * failure is reported, not thrown.
   */
  add(task: ScopedTask): Promise<void> {
    const signal = this.#controller.signal;
    // The task starts now, so it sees an abort that follows at once.
    const run = (async () => {
      try {
        await task(signal);
      } catch (error) {
        if (!signal.aborted) console.warn("Session task failed", error);
      }
    })().finally(() => {
      this.#tasks.delete(run);
    });
    this.#tasks.add(run);
    return run;
  }

  /** Registers teardown that runs (in order) when the session closes. */
  addCleanup(cleanup: ScopedCleanup): void {
    if (this.#closed) {
      void Promise.resolve().then(cleanup).catch(report);
      return;
    }
    this.#cleanups.push(cleanup);
  }

  /** Registers teardown that runs after every task and cleanup finished. */
  addFinalCleanup(cleanup: ScopedCleanup): void {
    if (this.#closed) {
      void Promise.resolve().then(cleanup).catch(report);
      return;
    }
    this.#finalCleanups.push(cleanup);
  }

  /**
   * Aborts every task, runs the cleanups one at a time, awaits every task,
   * then runs the final cleanups. A second call awaits nothing new.
   */
  async cancelAndWait(): Promise<void> {
    this.#closed = true;
    const cleanups = this.#cleanups;
    const finals = this.#finalCleanups;
    this.#cleanups = [];
    this.#finalCleanups = [];
    this.#controller.abort();
    for (const cleanup of cleanups) {
      try {
        await cleanup();
      } catch (error) {
        report(error);
      }
    }
    while (this.#tasks.size > 0) {
      await Promise.allSettled(this.#tasks);
    }
    for (const cleanup of finals) {
      try {
        await cleanup();
      } catch (error) {
        report(error);
      }
    }
  }
}
