import { expect, it } from "vitest";

import { ReaderStream } from "../../../../target/sdk-generated/typescript-wasm/runtime/streams/reader.js";

function latch() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

export function registerStreamTests(): void {
  // verifies: PROC-052
  it("rejects a second read until the first reaches the app", async () => {
    let inFlight = 0;
    let most = 0;
    const releases: (() => void)[] = [];
    let value = 0;
    const reader = {
      async next(): Promise<number | undefined> {
        inFlight++;
        most = Math.max(most, inFlight);
        const release = latch();
        releases.push(release.resolve);
        await release.promise;
        inFlight--;
        return ++value;
      },
      async end(): Promise<void> {},
    };
    const stream = new ReaderStream(async () => reader, {});
    const first = stream.next();
    await expect(stream.next()).rejects.toThrow(
      "reader iterator read is active",
    );
    await new Promise<void>((resolve) => setTimeout(resolve, 10));
    expect(most).toBe(1);
    releases.shift()!();
    expect(await first).toEqual({ done: false, value: 1 });
    const second = stream.next();
    await new Promise<void>((resolve) => setTimeout(resolve, 10));
    releases.shift()!();
    expect(await second).toEqual({ done: false, value: 2 });
    expect(most).toBe(1);
    await stream.end();
  });

  // A pending read after end of stream does not wait for the reader's end.
  it("answers one pending read at once when the stream closes", async () => {
    const entered = latch();
    const reader = {
      async next(): Promise<number | undefined> {
        entered.resolve();
        return new Promise<number | undefined>(() => {});
      },
      end(): Promise<void> {
        return new Promise<void>(() => {});
      },
    };
    const stream = new ReaderStream(async () => reader, {});
    const pending = stream.next();
    await entered.promise;
    void stream.end();
    const result = await Promise.race([
      pending,
      new Promise<string>((resolve) => setTimeout(() => resolve("hung"), 200)),
    ]);
    expect(result).toEqual({ done: true, value: undefined });
  });
}
