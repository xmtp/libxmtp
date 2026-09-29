import { bridgeError } from "../wire.js";

interface Call {
  started: Promise<void>;
  // An owner end waits for this. It is `done`, except for a read that is
  // abandoned at end: that read drains when its database work settles.
  drained: Promise<void>;
}

/** Tracks accepted calls before their binding futures start. */
export class OwnerCalls {
  private readonly active = new Map<number, Set<Call>>();
  private readonly closing = new Set<number>();

  accept(
    owner: number,
    abandonedAtEnd = false,
  ): { start(): void; settle(): void; finish(): void } {
    if (this.closing.has(owner)) throw bridgeError("clientClosed");
    let start!: () => void;
    let settle!: () => void;
    let finish!: () => void;
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
    };
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
        calls.delete(call);
        if (calls.size === 0) this.active.delete(owner);
      },
    };
  }

  fence(owner: number): { started: Promise<void>; drained: Promise<void> } {
    if (this.closing.has(owner)) throw bridgeError("clientClosed");
    this.closing.add(owner);
    const calls = [...(this.active.get(owner) ?? [])];
    return {
      started: Promise.all(calls.map((call) => call.started)).then(() => {}),
      drained: Promise.all(calls.map((call) => call.drained)).then(() => {}),
    };
  }

  reopen(owner: number): void {
    this.closing.delete(owner);
  }

  forget(owner: number): void {
    this.closing.delete(owner);
  }
}
