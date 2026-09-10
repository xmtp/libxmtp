import { describe, expect, it } from "vitest";
import { createRegisteredClient, createSigner } from "@test/helpers";

const nextWithin = async <T>(stream: {
  next(): Promise<IteratorResult<T, undefined>>;
}): Promise<IteratorResult<T, undefined>> => {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      stream.next(),
      new Promise<never>((_, reject) => {
        timer = setTimeout(
          () => reject(new Error("The message delivery deadline expired")),
          10000,
        );
      }),
    ]);
  } finally {
    if (timer !== undefined) clearTimeout(timer);
  }
};

describe("durable message delivery", () => {
  it("redelivers an unacknowledged item and keeps replay independent", async () => {
    const { signer } = createSigner();
    const client = await createRegisteredClient(signer);
    const streams: Array<{ end(): Promise<unknown> }> = [];
    try {
      const group = await client.conversations.createGroup([]);
      const firstId = await group.sendText("first retained message");
      const secondId = await group.sendText("second retained message");
      const history = await group.messageHistorySnapshot(128);
      const original = await group.stream();
      streams.push(original);
      const firstPosition = history.messages.findIndex(
        ({ message }) => message.id === firstId,
      );
      expect(firstPosition).toBeGreaterThanOrEqual(0);
      let first = await nextWithin(original);
      for (let position = 0; position <= firstPosition; position++) {
        if (position > 0) first = await nextWithin(original);
        expect(first.value?.id).toBe(history.messages[position]?.message.id);
        expect(first.value?.deliveryCursor).toEqual(
          history.messages[position]?.cursor,
        );
      }
      expect(first.value?.id).toBe(firstId);
      const cursor = first.value?.deliveryCursor;
      expect(cursor).toBeDefined();
      await original.end();

      const resumed = await group.stream();
      streams.push(resumed);
      const repeated = await nextWithin(resumed);
      expect(repeated.value?.id).toBe(firstId);
      expect(repeated.value?.deliveryCursor).toEqual(cursor);
      expect((await nextWithin(resumed)).value?.id).toBe(secondId);
      await resumed.end();

      const replay = await group.stream({ from: cursor });
      streams.push(replay);
      expect((await nextWithin(replay)).value?.id).toBe(secondId);
      await replay.end();

      const unchanged = await group.stream();
      streams.push(unchanged);
      expect((await nextWithin(unchanged)).value?.id).toBe(secondId);
      await unchanged.end();
    } finally {
      await Promise.allSettled(streams.map((stream) => stream.end()));
      await client.close();
    }
  });

  it("starts after the history snapshot without losing later messages", async () => {
    const { signer } = createSigner();
    const client = await createRegisteredClient(signer);
    let closeStream: (() => Promise<unknown>) | undefined;
    try {
      const group = await client.conversations.createGroup([]);
      const firstId = await group.sendText("history message");
      const history = await group.messageHistorySnapshot(128);
      expect(
        history.messages.some(({ message }) => message.id === firstId),
      ).toBe(true);
      const secondId = await group.sendText("after the snapshot");
      const thirdId = await group.sendText("also after the snapshot");
      const stream = await group.stream({ from: history.cursor });
      closeStream = stream.end;
      expect((await nextWithin(stream)).value?.id).toBe(secondId);
      expect((await nextWithin(stream)).value?.id).toBe(thirdId);
      expect(stream.deliveredCursor?.deliverySequence).toBeGreaterThan(
        history.cursor.deliverySequence,
      );
    } finally {
      await closeStream?.();
      await client.close();
    }
  });
});
