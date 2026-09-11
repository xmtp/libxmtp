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
        : Promise.reject(
            new Error("[LocalDeliveryError::SelectionChanged] stale token"),
          ),
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
  it("propagates a codec failure and leaves the message replayable", async () => {
    const bad = makeItem("1");
    const good = makeItem("2");
    const { reader } = makeReader([bad, good]);

    const stream = new MessageStream<Item, Item>(reader, (message) => {
      if (message.id === "1") throw new Error("codec exploded");
      return message;
    });

    // A message this client cannot decode must not be skipped. Acknowledging
    // it would advance the durable cursor past a retained message that
    // nothing has read, so a later reader with the codec registered would
    // never see it.
    await expect(stream.next()).rejects.toThrow("codec exploded");
    expect(bad.acknowledge).not.toHaveBeenCalled();
    expect(good.acknowledge).not.toHaveBeenCalled();
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
