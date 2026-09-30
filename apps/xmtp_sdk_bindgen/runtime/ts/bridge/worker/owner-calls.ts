import { bridgeError } from "../wire.js";

interface Call {
  started: Promise<void>;
  // An owner end waits for this. It is `done`, except for a read that is
  // abandoned at end: that read drains when its database work settles.
  drained: Promise<void>;
  // Resolves when the call waits on a host callback. The host code may itself
  // wait for Client.end, so a parked call does not hold up the end reply.
  parked: Promise<void>;
  // Resolves when the call has finished. The owner's storage lock is
  // released only after every call has finished.
  done: Promise<void>;
}

/** Tracks accepted calls before their binding futures start. */
export class OwnerCalls {
  private readonly active = new Map<number, Set<Call>>();
  private readonly closing = new Set<number>();
  // Host callback handles passed as arguments to an active call.
  private readonly parkers = new Map<number, () => void>();

  accept(
    owner: number,
    abandonedAtEnd = false,
    callbackHandles: readonly number[] = [],
  ): { start(): void; settle(): void; finish(): void } {
    if (this.closing.has(owner)) throw bridgeError("clientClosed");
    let start!: () => void;
    let settle!: () => void;
    let finish!: () => void;
    let park!: () => void;
    const settled = new Promise<void>((resolve) => {
      settle = resolve;
    });
    const done = new Promise<void>((resolve) => {
      finish = resolve;
    });
    const call: Call = {
      started: new Promise<void>((resolve) => {
        start = resolve;
      }),
      drained: abandonedAtEnd ? settled : done,
      parked: new Promise<void>((resolve) => {
        park = resolve;
      }),
      done,
    };
    for (const cb of callbackHandles) this.parkers.set(cb, park);
    let calls = this.active.get(owner);
    if (!calls) {
      calls = new Set();
      this.active.set(owner, calls);
    }
    calls.add(call);
    return {
      start,
      settle,
      finish: () => {
        start();
        settle();
        finish();
        for (const cb of callbackHandles) this.parkers.delete(cb);
        calls.delete(call);
        if (calls.size === 0) this.active.delete(owner);
      },
    };
  }

  /** Records that a call invoked a host callback it received as an argument. */
  parked(cb: number): void {
    this.parkers.get(cb)?.();
  }

  fence(owner: number): {
    started: Promise<void>;
    drained: Promise<void>;
    done: Promise<void>;
  } {
    if (this.closing.has(owner)) throw bridgeError("clientClosed");
    this.closing.add(owner);
    const calls = [...(this.active.get(owner) ?? [])];
    return {
      started: Promise.all(calls.map((call) => call.started)).then(() => {}),
      drained: Promise.all(
        calls.map((call) => Promise.race([call.drained, call.parked])),
      ).then(() => {}),
      done: Promise.all(calls.map((call) => call.done)).then(() => {}),
    };
  }

  reopen(owner: number): void {
    this.closing.delete(owner);
  }

  forget(owner: number): void {
    this.closing.delete(owner);
  }
}
