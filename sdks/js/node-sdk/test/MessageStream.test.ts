import { describe, expect, it, vi } from "vitest";
import { MessageStream, type MessageReaderSource } from "@/MessageStream";

type Item = { id: string };

const cursor = (deliverySequence: bigint) => ({
  deliverySequence,
  databaseId: new Uint8Array(16),
}) as never;

// One acknowledgeable delivery, recording which terminal action it received.
const makeItem = (id: string) => {
  const acknowledge = vi.fn().mockResolvedValue(undefined);
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
        checkOwner: () => true,
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

  it("skips a filtered message quietly, without reporting an error", async () => {
    const skipped = makeItem("1");
    const good = makeItem("2");
    const { reader } = makeReader([skipped, good]);
    const onError = vi.fn();

    const stream = new MessageStream<Item, Item>(
      reader,
      (message) => (message.id === "1" ? undefined : message),
      { onError },
    );

    const first = await stream.next();
    expect(first.value?.id).toBe("2");
    expect(skipped.acknowledge).toHaveBeenCalledTimes(1);
    expect(skipped.reject).not.toHaveBeenCalled();
    // Exclusion by the converter is intentional filtering, not a failure.
    expect(onError).not.toHaveBeenCalled();
    expect(stream.isDone).toBe(false);
  });
});
