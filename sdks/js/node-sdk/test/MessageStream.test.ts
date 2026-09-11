import { describe, expect, it, vi } from "vitest";
import { MessageStream, type MessageReaderSource } from "@/MessageStream";

type Item = { id: string };

const cursor = (deliverySequence: bigint) => ({
  deliverySequence,
  databaseId: new Uint8Array(16),
}) as never;

// One acknowledgeable delivery, recording which terminal action it received.
const makeItem = (id: string, owned = true) => {
  // Acknowledging a token the selection invalidated throws, matching the core.
  const acknowledge = vi
    .fn()
    .mockImplementation(() =>
      owned
        ? Promise.resolve(undefined)
        : Promise.reject(new Error("SelectionChanged")),
    );
  const reject = vi.fn().mockResolvedValue(undefined);
  return {
    acknowledge,
    reject,
    delivery: {
      message: { id },
      cursor: cursor(BigInt(id)),
      acknowledgement: {
        acknowledge,
        reject,
        checkOwner: () => owned,
      },
    },
  };
};

const makeReader = (items: ReturnType<typeof makeItem>[]) => {
  let index = 0;
  const close = vi.fn();
  const reader = {
    nextDelivery: () =>
      Promise.resolve(index < items.length ? items[index++]!.delivery : null),
    close,
    updateScope: () => {},
    updateFilter: () => {},
    catchUpSnapshot: () => null,
    catchUpChanged: () => Promise.resolve(null),
  } as unknown as MessageReaderSource<Item>;
  return { reader, close };
};

describe("MessageStream decode failures", () => {
  it("skips and acknowledges a message it cannot decode, then keeps delivering", async () => {
    const bad = makeItem("1");
    const good = makeItem("2");
    const { reader } = makeReader([bad, good]);
    const onError = vi.fn();

    const stream = new MessageStream<Item, Item>(
      reader,
      (message) => {
        if (message.id === "1") throw new Error("codec exploded");
        return message;
      },
      { onError },
    );

    // The undecodable message never reaches the caller, but the next one does.
    const first = await stream.next();
    expect(first.done).toBe(false);
    expect(first.value?.id).toBe("2");

    // It must be acknowledged, not rejected: a rejected item is re-served by
    // the core on every later stream, which would stop delivery permanently.
    expect(bad.acknowledge).toHaveBeenCalledTimes(1);
    expect(bad.reject).not.toHaveBeenCalled();
    expect(stream.isDone).toBe(false);

    // The failure is reported rather than silently swallowed.
    expect(onError).toHaveBeenCalledTimes(1);
    expect((onError.mock.calls[0]![0] as Error).message).toBe("codec exploded");
  });

  it("discards an undecodable item the selection invalidated, without ending the stream", async () => {
    // updateScope/updateFilter can invalidate the token while an async
    // converter runs. Acknowledging it then throws SelectionChanged, which
    // would terminate the stream — the exact failure the skip exists to avoid.
    const stale = makeItem("1", false);
    const good = makeItem("2");
    const { reader } = makeReader([stale, good]);
    const onError = vi.fn();

    const stream = new MessageStream<Item, Item>(
      reader,
      (message) => {
        if (message.id === "1") throw new Error("codec exploded");
        return message;
      },
      { onError },
    );

    const first = await stream.next();
    expect(first.done).toBe(false);
    expect(first.value?.id).toBe("2");
    // The failed acknowledgement is tolerated rather than ending the stream.
    // The core moved the item to reselect, so it stays replayable.
    expect(stale.acknowledge).toHaveBeenCalledTimes(1);
    expect(stream.isDone).toBe(false);
  });

  it("does not skip a message whose lookup failed", async () => {
    const missing = makeItem("1");
    const good = makeItem("2");
    const { reader } = makeReader([missing, good]);
    const onError = vi.fn();

    const stream = new MessageStream<Item, Item>(
      reader,
      (message) => (message.id === "1" ? undefined : message),
      { onError },
    );

    // undefined means the converter could not produce a value, which includes
    // a failed lookup. Acknowledging would advance the durable cursor past a
    // message nothing has read, so the stream stops and leaves it replayable.
    await expect(stream.next()).rejects.toThrow(/could not be decoded/);
    expect(missing.acknowledge).not.toHaveBeenCalled();
    expect(onError).toHaveBeenCalledTimes(1);
  });
});
