import type { HandleWire } from "../wire.js";
import type { MainSession } from "./session.js";

const collected = new FinalizationRegistry<{
  session: MainSession;
  handle: number;
}>((held) => {
  held.session.collected(held.handle);
});

let sessionOfObject: (proxy: RemoteObject) => MainSession;

export function sessionOf(proxy: RemoteObject): MainSession {
  return sessionOfObject(proxy);
}

let endOwnerOf: ((proxy: RemoteObject) => void) | undefined;

export class RemoteObject {
  private released = false;

  static {
    sessionOfObject = (proxy) => proxy.session;
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
    args: unknown[] | (() => unknown[]),
    signal?: AbortSignal,
  ): Promise<unknown> {
    if (
      (key === "EventReader.next" ||
        key === "EventReader.end" ||
        key === "Client.stopListener") &&
      this.session.eventReaderEnded(this.handle)
    )
      return Promise.resolve(undefined);
    if (
      (key !== "EventReader.next" &&
        key !== "EventReader.end" &&
        key !== "Client.stopListener") ||
      !this.session.eventReaderEnding(this.handle)
    )
      this.check();
    return this.session.call(key, args, this.handle, signal);
  }

  /**
   * Reads and decodes an immutable field from the snapshot that the handle
   * holds. It makes no worker call, so it stays readable after the owner
   * client ends, as the Node getters do (Decision 14). A nested object
   * decodes to a proxy whose calls fail with ClientClosed.
   */
  protected held<T>(read: () => T): T {
    return this.session.readHeld(read);
  }

  protected snapshot(name: string): unknown {
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
