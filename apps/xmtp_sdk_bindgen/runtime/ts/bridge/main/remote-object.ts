import type { HandleWire } from "../wire.js";
import type { MainSession } from "./session.js";

const collected = new FinalizationRegistry<{
  session: MainSession;
  handle: number;
}>((held) => {
  held.session.collected(held.handle);
});

let endOwnerOf: ((proxy: RemoteObject) => void) | undefined;

export class RemoteObject {
  private released = false;

  static {
    endOwnerOf = (proxy) => proxy.#endOwner();
  }

  constructor(
    protected readonly session: MainSession,
    readonly handle: HandleWire,
  ) {
    session.remember(this);
    collected.register(this, { session, handle: handle.h }, this);
  }

  protected call(
    key: string,
    args: unknown[],
    signal?: AbortSignal,
  ): Promise<unknown> {
    this.check();
    return this.session.call(key, args, this.handle, signal);
  }

  protected snapshot(name: string): unknown {
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
    return fields[name];
  }

  protected check(): void {
    if (this.released) throw this.session.error("clientClosed");
    this.session.checkHandle(this.handle);
  }

  checkLive(name: string, session?: MainSession): void {
    if (session !== this.session || this.handle.type !== name)
      throw this.session.error("clientClosed");
    this.check();
  }

  protected fence(): void {
    this.session.fenceOwner(this.handle.owner);
  }

  protected unfence(): void {
    this.session.unfenceOwner(this.handle.owner);
  }

  release(): void {
    if (this.released) return;
    this.released = true;
    collected.unregister(this);
    this.session.forget(this);
    this.session.collected(this.handle.h);
  }

  #endOwner(): void {
    this.released = true;
    collected.unregister(this);
    this.session.forget(this);
    this.session.closeOwner(this.handle.owner, [this.handle.h]);
  }
}

/**
 * Closes the owner of `proxy` after `Client.end` resolved. The worker then
 * drops the owner handles and releases its storage lock. Only the generated
 * `Client.end` calls this. It is not a proxy member, so app code cannot reach
 * it through an exported object.
 */
export function endOwner(proxy: RemoteObject): void {
  endOwnerOf?.(proxy);
}
