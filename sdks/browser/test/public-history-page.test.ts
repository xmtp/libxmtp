import { expect, test } from "vitest";

import { create } from "./helpers";

test("the real WASM worker lifts a history page and lowers both position bounds", async () => {
  const client = await create();
  const peer = await create();
  for (const conversation of [
    await client.conversations.createGroup([]),
    await client.conversations.createDm(peer.inboxId),
  ]) {
    const ids: MessageId[] = [];
    for (const text of ["first", "second", "third"])
      ids.push(await conversation.sendText(text));
    const first = await conversation.messageHistoryPage({
      limit: 2,
      kind: "application",
    });
    expect(first.messages.map((message) => message.id)).toEqual(
      ids.slice(0, 2),
    );
    expect(first.messages[0]?.content).toEqual({
      kind: "text",
      value: "first",
    });
    expect(first.skippedCount).toBe(0);
    expect(first.hasMore).toBe(true);
    expect(first.firstPosition?.sentAt).toEqual(first.messages[0]?.sentAt);
    expect(first.lastPosition?.deliveryCursor).toEqual(
      first.messages[1]?.deliveryCursor,
    );
    const next = await conversation.messageHistoryPage(
      { limit: 2, kind: "application" },
      undefined,
      first.lastPosition,
    );
    expect(next.messages.map((message) => message.id)).toEqual(ids.slice(2));
    expect(next.hasMore).toBe(false);
    const older = await conversation.messageHistoryPage(
      { limit: 2, kind: "application", direction: "descending" },
      next.firstPosition,
    );
    expect(older.messages.map((message) => message.id)).toEqual(
      ids.slice(0, 2).reverse(),
    );
    const defaults = await conversation.messageHistoryPage();
    expect(
      defaults.messages
        .filter((message) => ids.includes(message.id))
        .map((message) => message.id),
    ).toEqual(ids);
  }
});
import type { MessageId } from "@xmtp/browser-sdk";
