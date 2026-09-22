import { createRegisteredClient, createUser } from "@test/helpers";
import { describe, expect, it } from "vitest";

import type { MessageReader } from "../dist";

const within = async <T>(operation: Promise<T>) => {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      operation,
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

const nextWithin = (reader: MessageReader) => within(reader.nextDelivery());

describe("message reader constructors", () => {
  it.each(["enrichment", "ownership", "acknowledgement"] as const)(
    "fails %s and preserves the item for a caller-started reader",
    async (operation) => {
      const client = await createRegisteredClient(createUser());
      let reader: MessageReader | undefined;
      let replacement: MessageReader | undefined;
      try {
        const group = await client.conversations().createGroup([]);
        const history = group.messageHistorySnapshot(128);
        reader = await group.messageReader();
        for (const expected of history.messages) {
          const initial = await nextWithin(reader);
          expect(initial?.message.id).toBe(expected.message.id);
          initial?.acknowledgement.acknowledge();
        }
        const firstId = await group.sendText("before storage failure");
        const secondId = await group.sendText("after failed acknowledgement");
        const delivery = await nextWithin(reader);
        if (delivery === null) throw new Error("Expected a retained message");
        expect(delivery.message.id).toBe(firstId);
        const token = delivery.acknowledgement;

        client.releaseDbConnection();
        const fail = {
          enrichment: () => token.enrichedMessage(),
          ownership: () => token.checkOwner(),
          acknowledgement: () => token.acknowledge(),
        }[operation];
        expect(fail).toThrow();

        await client.dbReconnect();
        reader.close();
        replacement = await group.messageReader();
        const retained = await nextWithin(replacement);
        expect(retained?.message.id).toBe(firstId);
        expect(() => token.acknowledge()).toThrow();
        expect(retained?.acknowledgement.enrichedMessage()?.id).toBe(firstId);
        retained?.acknowledgement.acknowledge();
        const second = await nextWithin(replacement);
        expect(second?.message.id).toBe(secondId);
        second?.acknowledgement.acknowledge();
      } finally {
        await client.dbReconnect().catch(() => {});
        reader?.close();
        replacement?.close();
        await client.close();
      }
    },
  );

  it("awaits the all-groups constructor and reads retained rows with tokens", async () => {
    const client = await createRegisteredClient(createUser());
    let reader: MessageReader | undefined;
    try {
      const conversations = client.conversations();
      const firstGroup = await conversations.createGroup([]);
      const secondGroup = await conversations.createGroup([]);
      const firstId = await firstGroup.sendText("first group retained message");
      const secondId = await secondGroup.sendText(
        "second group retained message",
      );
      const history = conversations.messageHistorySnapshot(128);
      expect(history.messages.map(({ message }) => message.id)).toContain(
        firstId,
      );
      expect(history.messages.map(({ message }) => message.id)).toContain(
        secondId,
      );

      reader = await conversations.messageReader();
      expect(reader.catchUpSnapshot().current.scopeGeneration).toBeTypeOf(
        "bigint",
      );
      for (const expected of history.messages) {
        const delivery = await nextWithin(reader);
        if (delivery === null)
          throw new Error("The reader closed before delivery");
        expect(delivery.message).toEqual(expected.message);
        expect(delivery.cursor).toEqual(expected.cursor);
        const acknowledgement = delivery.acknowledgement;
        const enriched = acknowledgement.enrichedMessage();
        expect(enriched?.id).toBe(delivery.message.id);
        expect(acknowledgement.checkOwner()).toBe(true);
        acknowledgement.acknowledge();
      }
      reader.close();
      expect(await nextWithin(reader)).toBeNull();
    } finally {
      reader?.close();
      await client.close();
    }
  });

  it("awaits the single-group constructor and replays only that group", async () => {
    const client = await createRegisteredClient(createUser());
    let reader: MessageReader | undefined;
    try {
      const conversations = client.conversations();
      const group = await conversations.createGroup([]);
      const otherGroup = await conversations.createGroup([]);
      const history = group.messageHistorySnapshot(128);
      await otherGroup.sendText("outside the reader scope");
      const firstId = await group.sendText("first replay message");
      const secondId = await group.sendText("second replay message");

      reader = await group.messageReader(history.cursor);
      expect(reader.catchUpSnapshot().current.scopeGeneration).toBeTypeOf(
        "bigint",
      );
      let previousSequence = history.cursor.deliverySequence;
      for (const expectedId of [firstId, secondId]) {
        const delivery = await nextWithin(reader);
        if (delivery === null)
          throw new Error("The reader closed before delivery");
        expect(delivery.message.id).toBe(expectedId);
        expect(delivery.message.convoId).toBe(group.id());
        expect(delivery.cursor.databaseId).toEqual(history.cursor.databaseId);
        expect(delivery.cursor.deliverySequence).toBeGreaterThan(
          previousSequence,
        );
        previousSequence = delivery.cursor.deliverySequence;
        const acknowledgement = delivery.acknowledgement;
        expect(acknowledgement.checkOwner()).toBe(true);
        acknowledgement.acknowledge();
      }
      reader.close();
      expect(await nextWithin(reader)).toBeNull();
    } finally {
      reader?.close();
      await client.close();
    }
  });
});
