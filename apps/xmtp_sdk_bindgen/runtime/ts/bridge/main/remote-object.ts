import { bridgeError, type HandleWire } from "../wire.js";
import type { MainSession } from "./session.js";

const collected = new FinalizationRegistry<{
  session: MainSession;
  handle: number;
}>((held) => {
  held.session.release([held.handle]);
});

export class RemoteObject {
  private released = false;

  constructor(
    protected readonly session: MainSession,
    readonly handle: HandleWire,
  ) {
    collected.register(this, { session, handle: handle.h }, this);
  }

  protected call<T>(
    key: string,
    args: unknown[],
    signal?: AbortSignal,
  ): Promise<T> {
    this.check();
    return this.session.call<T>(key, args, this.handle, signal);
  }

  protected snapshot<T>(name: string): T {
    this.check();
    if (
      this.handle.snap === null ||
      typeof this.handle.snap !== "object" ||
      !(name in this.handle.snap)
    ) {
      throw new TypeError(`missing immutable field ${name}`);
    }
    const fields: Record<string, unknown> = Object.fromEntries(
      Object.entries(this.handle.snap),
    );
    return fields[name] as T;
  }

  protected check(): void {
    if (this.released) throw bridgeError("clientClosed");
    this.session.checkHandle(this.handle);
  }

  release(): void {
    if (this.released) return;
    this.released = true;
    collected.unregister(this);
    this.session.release([this.handle.h]);
  }

  endOwner(): void {
    this.released = true;
    collected.unregister(this);
    this.session.closeOwner(this.handle.owner, [this.handle.h]);
  }
}
