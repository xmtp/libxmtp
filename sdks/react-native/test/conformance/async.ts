/** A deadline diagnoses a wedged native callback; barriers decide ordering. */
export async function within<T>(
  promise: Promise<T>,
  label: string,
): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      promise,
      new Promise<never>((_, reject) => {
        timer = setTimeout(
          () => reject(new Error(`Timed out: ${label}`)),
          30_000,
        );
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}

export function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((settle) => {
    resolve = settle;
  });
  return { promise, resolve };
}

export class Mailbox<T> {
  private readonly queued: T[] = [];
  private readonly readers: Array<(value: T) => void> = [];
  push(value: T): void {
    const reader = this.readers.shift();
    if (reader) reader(value);
    else this.queued.push(value);
  }
  next(label: string): Promise<T> {
    return within(
      this.queued.length
        ? Promise.resolve(this.queued.shift()!)
        : new Promise<T>((resolve) => this.readers.push(resolve)),
      label,
    );
  }
}
