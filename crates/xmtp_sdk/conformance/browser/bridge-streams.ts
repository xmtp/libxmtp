import { expect, it } from "vitest";

import { MessageStream } from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/streams/reader.js";

function latch() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

export function registerStreamTests(): void {
  // verifies: PROC-052
  it("runs one underlying read at a time for concurrent stream reads", async () => {
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
    const stream = new MessageStream(async () => reader, {});
    const first = stream.next();
    const second = stream.next();
    await new Promise<void>((resolve) => setTimeout(resolve, 10));
    expect(most).toBe(1);
    releases.shift()!();
    expect(await first).toEqual({ done: false, value: 1 });
    await new Promise<void>((resolve) => setTimeout(resolve, 10));
    releases.shift()!();
    expect(await second).toEqual({ done: false, value: 2 });
    expect(most).toBe(1);
    await stream.end();
  });

  // A queued read after end of stream does not wait for the reader's end.
  it("answers a queued read at once when the stream closes", async () => {
    const reader = {
      async next(): Promise<number | undefined> {
        return undefined;
      },
      end(): Promise<void> {
        return new Promise<void>(() => {});
      },
    };
    const stream = new MessageStream(async () => reader, {});
    void stream.next();
    const second = await Promise.race([
      stream.next(),
      new Promise<string>((resolve) => setTimeout(() => resolve("hung"), 200)),
    ]);
    expect(second).toEqual({ done: true, value: undefined });
  });
}
