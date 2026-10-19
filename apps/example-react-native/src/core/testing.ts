// Test support for `node --test`. Only tests import it. Fakes resolve at
// once, so one macrotask turn runs every promise callback they queued.

/** Waits one macrotask turn, after every queued promise callback ran. */
export function settle(): Promise<void> {
  return new Promise((resolve) => setImmediate(resolve));
}

export interface Deferred<T> {
  readonly promise: Promise<T>;
  readonly resolve: (value: T) => void;
  readonly reject: (error: unknown) => void;
}

/** A promise that the test settles, to hold a step until it says so. */
export function deferred<T = void>(): Deferred<T> {
  let resolve: (value: T) => void = () => undefined;
  let reject: (error: unknown) => void = () => undefined;
  const promise = new Promise<T>((onResolve, onReject) => {
    resolve = onResolve;
    reject = onReject;
  });
  return { promise, resolve, reject };
}
