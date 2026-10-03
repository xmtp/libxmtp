import { expect, test, vi } from "vitest";

import { ReaderStream } from "../../../apps/xmtp_sdk_bindgen/runtime/ts/streams/reader";

function heldReader() {
  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  let reads = 0;
  const stream = new ReaderStream(
    async () => ({
      next: async () => {
        reads++;
        return reads <= 2 ? reads : undefined;
      },
      end: async () => undefined,
    }),
    {},
  );
  return { stream, held, release, reads: () => reads };
}

test("a second callback consumer cannot acknowledge a held value", async () => {
  const { stream, held, release, reads } = heldReader();
  const seen: number[] = [];
  const first = stream.onValue(async (value) => {
    seen.push(value);
    if (value === 1) await held;
  });
  try {
    await vi.waitFor(() => expect(reads()).toBe(1));
    await expect(stream.onValue(() => undefined)).rejects.toThrow(
      /callback consumer/,
    );
    expect(reads()).toBe(1);
  } finally {
    release();
    await first;
    await stream.end();
  }
  expect(seen).toEqual([1, 2]);
});

test("iterator reads cannot acknowledge a held callback value", async () => {
  const { stream, held, release, reads } = heldReader();
  const first = stream.onValue(async (value) => {
    if (value === 1) await held;
  });
  try {
    await vi.waitFor(() => expect(reads()).toBe(1));
    await expect(stream.next()).rejects.toThrow(/callback consumer/);
    expect(reads()).toBe(1);
  } finally {
    release();
    await first;
    await stream.end();
  }
});

test("a second iterator cannot acknowledge a held iterator value", async () => {
  const { stream, reads } = heldReader();
  const first = stream[Symbol.asyncIterator]();
  try {
    await expect(first.next()).resolves.toEqual({ done: false, value: 1 });
    await expect(
      (async () => {
        const second = stream[Symbol.asyncIterator]();
        return second.next();
      })(),
    ).rejects.toThrow(/iterator consumer/);
    await expect(stream.next()).rejects.toThrow(/iterator consumer/);
    expect(reads()).toBe(1);
    await expect(first.next()).resolves.toEqual({ done: false, value: 2 });
  } finally {
    await stream.end();
  }
});

test("a callback cannot take a stream after an iterator read starts", async () => {
  let releaseRead!: (value: number) => void;
  const firstValue = new Promise<number>((resolve) => {
    releaseRead = resolve;
  });
  let reads = 0;
  const stream = new ReaderStream(
    async () => ({
      next: async () => {
        reads++;
        return reads === 1 ? firstValue : undefined;
      },
      end: async () => undefined,
    }),
    {},
  );
  const callback = vi.fn();
  const first = stream.next();
  try {
    await vi.waitFor(() => expect(reads).toBe(1));
    const blocked = expect(stream.onValue(callback)).rejects.toThrow(
      /iterator consumer/,
    );
    releaseRead(7);
    await expect(first).resolves.toEqual({ done: false, value: 7 });
    await blocked;
    expect(reads).toBe(1);
    expect(callback).not.toHaveBeenCalled();
  } finally {
    releaseRead(7);
    await stream.end();
  }
});

test("a closed stream answers an iterator while a callback is held", async () => {
  const { stream, held, release, reads } = heldReader();
  const first = stream.onValue(async (value) => {
    if (value === 1) await held;
  });
  try {
    await vi.waitFor(() => expect(reads()).toBe(1));
    await stream.end();
    await expect(stream.next()).resolves.toEqual({
      done: true,
      value: undefined,
    });
  } finally {
    release();
    await first;
  }
});

// verifies: PROC-041
test("an acknowledgement failure ends callbacks and a new reader can replay", async () => {
  const storageError = new Error("acknowledgement storage failure");
  const closeReasons: unknown[] = [];
  const received: number[] = [];
  let reads = 0;
  let ends = 0;
  const failed = new ReaderStream(
    async () => ({
      next: async () => {
        reads++;
        if (reads === 1) return 1;
        throw storageError;
      },
      end: async () => {
        ends++;
      },
    }),
    {},
    { onClose: (reason) => closeReasons.push(reason) },
  );

  await expect(
    failed.onValue((value) => {
      received.push(value);
    }),
  ).rejects.toBe(storageError);
  expect(received).toEqual([1]);
  expect(reads).toBe(2);
  expect(ends).toBe(1);
  expect(closeReasons).toEqual([{ kind: "failed", error: storageError }]);
  await expect(failed.next()).rejects.toBe(storageError);

  let replayReads = 0;
  const replacement = new ReaderStream(
    async () => ({
      next: async () => {
        replayReads++;
        return replayReads <= 2 ? replayReads : undefined;
      },
      end: async () => undefined,
    }),
    {},
  );
  try {
    await failed.end();
    await expect(replacement.next()).resolves.toEqual({
      done: false,
      value: 1,
    });
    await expect(replacement.next()).resolves.toEqual({
      done: false,
      value: 2,
    });
    await expect(failed.next()).rejects.toBe(storageError);
  } finally {
    await replacement.end();
  }
});

test.each(["direct", "adapter"] as const)(
  "%s iterator rejects a second read before the first value reaches the app",
  async (mode) => {
    let release!: (value: number) => void;
    const firstValue = new Promise<number>((resolve) => {
      release = resolve;
    });
    let reads = 0;
    const stream = new ReaderStream(
      async () => ({
        next: async () => {
          reads++;
          return reads === 1 ? firstValue : reads === 2 ? 2 : undefined;
        },
        end: async () => undefined,
      }),
      {},
    );
    const iterator =
      mode === "adapter" ? stream[Symbol.asyncIterator]() : stream;
    const first = iterator.next();
    try {
      await vi.waitFor(() => expect(reads).toBe(1));
      const second = iterator.next();
      release(1);
      await expect(first).resolves.toEqual({ done: false, value: 1 });
      await expect(second).rejects.toThrow(/iterator read is active/);
      expect(reads).toBe(1);
      await expect(iterator.next()).resolves.toEqual({ done: false, value: 2 });
    } finally {
      release(1);
      await stream.end();
    }
  },
);
