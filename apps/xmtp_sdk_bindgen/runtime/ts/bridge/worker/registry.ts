import { bridgeError, type HandleWire } from "../wire.js";

interface Entry {
  value: object;
  owner: number;
  type: string;
}

export class WorkerRegistry {
  private readonly entries = new Map<number, Entry>();
  private readonly ownerCounts = new Map<number, number>();
  private readonly ownerClients = new Map<number, object>();
  private nextHandle = 1;
  private nextOwner = 1;
  // Handles that the current snapshot allocated. A snapshot can add nested
  // handles; if it throws, all of them are released with the parent handle.
  private allocations?: number[];

  constructor(readonly epoch: number) {}

  add(
    value: object,
    type: string,
    owner?: number,
    snapshot?: (owner: number) => unknown,
  ): HandleWire {
    const h = this.nextHandle++;
    const actualOwner = owner ?? this.nextOwner++;
    this.entries.set(h, { value, owner: actualOwner, type });
    if (type === "Client") this.ownerClients.set(actualOwner, value);
    this.ownerCounts.set(
      actualOwner,
      (this.ownerCounts.get(actualOwner) ?? 0) + 1,
    );
    const parent = this.allocations;
    const allocations = [h];
    this.allocations = allocations;
    try {
      const snap = snapshot?.(actualOwner);
      parent?.push(...allocations);
      return { h, owner: actualOwner, epoch: this.epoch, type, snap };
    } catch (error) {
      this.rollback(allocations);
      throw error;
    } finally {
      this.allocations = parent;
    }
  }

  /**
   * Encodes one result. Every handle added while `encode` runs, including
   * sibling handles in a list or record, is released if it throws.
   */
  scope<T>(encode: () => T): T {
    const parent = this.allocations;
    const allocations: number[] = [];
    this.allocations = allocations;
    try {
      const value = encode();
      parent?.push(...allocations);
      return value;
    } catch (error) {
      this.rollback(allocations);
      throw error;
    } finally {
      this.allocations = parent;
    }
  }

  private rollback(handles: number[]): void {
    for (const h of handles) {
      const entry = this.entries.get(h);
      if (!entry) continue;
      this.entries.delete(h);
      this.decrementOwner(entry.owner);
      if (
        entry.type === "Client" &&
        this.ownerClients.get(entry.owner) === entry.value
      )
        this.ownerClients.delete(entry.owner);
    }
  }

  get(handle: HandleWire): object {
    const entry = this.entries.get(handle.h);
    if (
      handle.epoch !== this.epoch ||
      !entry ||
      entry.owner !== handle.owner ||
      entry.type !== handle.type
    ) {
      throw bridgeError("clientClosed");
    }
    return entry.value;
  }

  release(handles: number[]): number[] {
    const closedOwners = new Set<number>();
    for (const h of handles) {
      const entry = this.entries.get(h);
      if (!entry) continue;
      this.entries.delete(h);
      if (this.decrementOwner(entry.owner)) closedOwners.add(entry.owner);
    }
    return [...closedOwners];
  }

  closeOwner(owner: number): void {
    for (const [h, entry] of this.entries) {
      if (entry.owner === owner) this.entries.delete(h);
    }
    this.ownerCounts.delete(owner);
    this.ownerClients.delete(owner);
  }

  takeClient(owner: number): object | undefined {
    const client = this.ownerClients.get(owner);
    this.ownerClients.delete(owner);
    return client;
  }

  private decrementOwner(owner: number): boolean {
    const count = this.ownerCounts.get(owner);
    if (count === undefined) return false;
    if (count > 1) {
      this.ownerCounts.set(owner, count - 1);
      return false;
    }
    this.ownerCounts.delete(owner);
    return true;
  }

  get size(): number {
    return this.entries.size;
  }
}
