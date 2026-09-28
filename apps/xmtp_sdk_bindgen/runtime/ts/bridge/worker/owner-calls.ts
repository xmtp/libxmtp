import { bridgeError } from "../wire.js";

interface Call {
  started: Promise<void>;
  done: Promise<void>;
}

/** Tracks accepted calls before their binding futures start. */
export class OwnerCalls {
  private readonly active = new Map<number, Set<Call>>();
  private readonly closing = new Set<number>();

  accept(owner: number): { start(): void; finish(): void } {
    if (this.closing.has(owner)) throw bridgeError("clientClosed");
    let start!: () => void;
    let finish!: () => void;
    const call: Call = {
      started: new Promise<void>((resolve) => {
        start = resolve;
      }),
      done: new Promise<void>((resolve) => {
        finish = resolve;
      }),
    };
    let calls = this.active.get(owner);
    if (!calls) {
      calls = new Set();
      this.active.set(owner, calls);
    }
    calls.add(call);
    return {
      start,
      finish: () => {
        start();
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
      drained: Promise.all(calls.map((call) => call.done)).then(() => {}),
    };
  }

  reopen(owner: number): void {
    this.closing.delete(owner);
  }

  forget(owner: number): void {
    this.closing.delete(owner);
  }
}
