import { createRegisteredClient, createSigner } from "@test/helpers";
import { Dm, MessageStream } from "@xmtp/node-sdk";
import { describe, expect, it } from "vitest";

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
      const thirdId = await group.sendText("third retained message");
      const history = await group.messageHistorySnapshot(128);
      const original = MessageStream.openGroup(client, group);
      await original.ready();
      streams.push(original);
      const firstPosition = history.messages.findIndex(
        (message) => message.id === firstId,
      );
      expect(firstPosition).toBeGreaterThanOrEqual(0);
      let first = await nextWithin(original);
      for (let position = 0; position <= firstPosition; position++) {
        if (position > 0) first = await nextWithin(original);
        expect(first.value?.id).toBe(history.messages[position]?.id);
        expect(first.value?.deliveryCursor).toEqual(
          history.messages[position]?.deliveryCursor,
        );
      }
      expect(first.value?.id).toBe(firstId);
      const cursor = first.value?.deliveryCursor;
      expect(typeof cursor).toBe("string");
      if (typeof cursor !== "string")
        throw new Error("Expected a delivery cursor");
      await original.end();

      const resumed = MessageStream.openGroup(client, group);
      await resumed.ready();
      streams.push(resumed);
      const repeated = await nextWithin(resumed);
      expect(repeated.value?.id).toBe(firstId);
      expect(repeated.value?.deliveryCursor).toEqual(cursor);
      expect((await nextWithin(resumed)).value?.id).toBe(secondId);
      await resumed.end();

      const replay = MessageStream.openGroup(client, group, { from: cursor });
      await replay.ready();
      streams.push(replay);
      expect((await nextWithin(replay)).value?.id).toBe(secondId);
      expect((await nextWithin(replay)).value?.id).toBe(thirdId);
      await replay.end();

      const unchanged = MessageStream.openGroup(client, group);
      await unchanged.ready();
      streams.push(unchanged);
      expect((await nextWithin(unchanged)).value?.id).toBe(secondId);
      await unchanged.end();
    } finally {
      await Promise.allSettled(streams.map((stream) => stream.end()));
      await client.end();
    }
  });

  it.each(["group", "dm", "all"] as const)(
    "starts %s delivery after the history snapshot without losing later messages",
    async (scope) => {
      const { signer } = createSigner();
      const client = await createRegisteredClient(signer);
      const peer =
        scope === "dm"
          ? await createRegisteredClient(createSigner().signer)
          : undefined;
      let closeStream: (() => Promise<unknown>) | undefined;
      try {
        const group = peer
          ? await client.conversations.createDm(peer.inboxId)
          : await client.conversations.createGroup([]);
        const firstId = await group.sendText("history message");
        const history =
          scope === "all"
            ? await client.conversations.messageHistorySnapshot(128, {
                conversationKind: "group",
              })
            : await group.messageHistorySnapshot(128);
        expect(history.messages.some((message) => message.id === firstId)).toBe(
          true,
        );
        const secondId = await group.sendText("after the snapshot");
        const thirdId = await group.sendText("also after the snapshot");
        const stream =
          scope === "all"
            ? MessageStream.open(client, {
                from: history.cursor,
                conversationKind: "group",
              })
            : group instanceof Dm
              ? MessageStream.openDm(client, group, { from: history.cursor })
              : MessageStream.openGroup(client, group, {
                  from: history.cursor,
                });
        await stream.ready();
        closeStream = () => stream.end();
        expect((await nextWithin(stream)).value?.id).toBe(secondId);
        const third = await nextWithin(stream);
        expect(third.value?.id).toBe(thirdId);
        expect(third.value?.deliveryCursor).toBeDefined();
        expect(third.value?.deliveryCursor).not.toBe(history.cursor);
      } finally {
        await closeStream?.();
        await client.end();
        await peer?.end();
      }
    },
  );
});
