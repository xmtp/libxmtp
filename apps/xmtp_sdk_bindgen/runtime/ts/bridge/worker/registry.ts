import { bridgeError, type HandleWire } from "../wire.js";

interface Entry {
  value: object;
  owner: number;
  type: string;
}

export class WorkerRegistry {
  private readonly entries = new Map<number, Entry>();
  private nextHandle = 1;
  private nextOwner = 1;

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
    try {
      return {
        h,
        owner: actualOwner,
        epoch: this.epoch,
        type,
        snap: snapshot?.(actualOwner),
      };
    } catch (error) {
      this.entries.delete(h);
      throw error;
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
      if (entry?.type === "Client") closedOwners.add(entry.owner);
      this.entries.delete(h);
    }
    return [...closedOwners];
  }

  closeOwner(owner: number): void {
    for (const [h, entry] of this.entries) {
      if (entry.owner === owner) this.entries.delete(h);
    }
  }

  get size(): number {
    return this.entries.size;
  }
}
