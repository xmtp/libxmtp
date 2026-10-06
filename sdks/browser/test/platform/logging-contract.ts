/** Tests the public async sink through the caller's real generated transport. */
export interface ContractLogRecord {
  readonly fields: ReadonlyMap<string, string>;
  readonly droppedRecords: bigint;
}

export interface LoggingApi {
  setLogSink(sink?: {
    log(record: ContractLogRecord): Promise<void>;
  }): Promise<void>;
  emit(count: number): Promise<void>;
}

class Mailbox<T> {
  private readonly queued: T[] = [];
  private readonly readers: Array<(value: T) => void> = [];
  push(value: T): void {
    const reader = this.readers.shift();
    if (reader) reader(value);
    else this.queued.push(value);
  }
  next(): Promise<T> {
    if (this.queued.length) return Promise.resolve(this.queued.shift()!);
    return new Promise((resolve) => this.readers.push(resolve));
  }
}

function equal<T>(actual: T, expected: T, label: string): void {
  if (actual !== expected)
    throw new Error(`${label}: ${String(actual)} != ${String(expected)}`);
}

// verifies: LOG-002, LOG-003, LOG-004, LOG-005, LOG-007, LOG-008, LOG-009,
// verifies: LOG-011, LOG-012, LOG-013
export async function loggingContract(api: LoggingApi): Promise<void> {
  const records = new Mailbox<ContractLogRecord>();
  const responses = new Mailbox<boolean>();
  let active = 0;
  let maximum = 0;
  await api.setLogSink({
    async log(record) {
      active++;
      maximum = Math.max(maximum, active);
      records.push(record);
      try {
        if (!(await responses.next())) {
          // Error formatting must not call an app object's toString.
          throw {
            toString() {
              throw new Error("hostile log error formatting");
            },
          };
        }
      } finally {
        active--;
      }
    },
  });
  await api.emit(1);
  equal((await records.next()).droppedRecords, 0n, "first dispatch");
  // The first call has reached the app and now holds a deterministic barrier.
  await api.emit(4099);
  responses.push(true);
  const second = await records.next();
  equal(second.fields.get("sequence"), "0", "admission order");
  equal(second.droppedRecords, 3n, "4096 queued plus one active");
  // Fill one free queue slot, then make two drops after this dispatch.
  await api.emit(3);
  responses.push(false);
  const third = await records.next();
  equal(third.fields.get("sequence"), "1", "failure does not reorder");
  equal(third.droppedRecords, 5n, "failed report retains its drops");
  await api.emit(2);
  responses.push(true);
  const fourth = await records.next();
  equal(fourth.droppedRecords, 1n, "successful report keeps later drops");
  equal(maximum, 1, "one active callback");

  const replacementRecords = new Mailbox<number>();
  for (let generation = 1; generation <= 100; generation++) {
    await api.setLogSink();
    await api.setLogSink({
      async log() {
        equal(active, 0, "replacement waits for old callback");
        replacementRecords.push(generation);
      },
    });
    await api.emit(1);
  }
  equal(active, 1, "replace does not wait for the old callback");
  responses.push(false);
  equal(
    await replacementRecords.next(),
    100,
    "only the final generation is handed off",
  );
  await api.setLogSink();

  const reentered = new Mailbox<void>();
  const nextGeneration = new Mailbox<void>();
  let oldFinished = false;
  await api.setLogSink({
    async log() {
      await api.setLogSink({
        async log() {
          equal(
            oldFinished,
            true,
            "new generation must wait for reentrant callback",
          );
          nextGeneration.push();
        },
      });
      await api.emit(1);
      oldFinished = true;
      reentered.push();
    },
  });
  await api.emit(1);
  await reentered.next();
  await nextGeneration.next();
  await api.setLogSink();
}
