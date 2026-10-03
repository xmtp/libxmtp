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
