import { expect, it } from "vitest";

import {
  Client,
  type MessageHistoryPage,
  type MessageHistoryPosition,
} from "@/index";
import { createSigner, createUser } from "@/user/User";

it("the Agent root exposes the native page records and continuation", async () => {
  const client = await Client.create(createSigner(createUser()), {
    backend: { url: process.env.XMTP_BACKEND_URL! },
    storage: { location: "inMemory" },
    deviceSync: false,
  });
  try {
    const group = await client.conversations.createGroup([]);
    const firstId = await group.sendText("first Agent history message");
    const secondId = await group.sendText("second Agent history message");
    const first: MessageHistoryPage = await group.messageHistoryPage({
      limit: 1,
      kind: "application",
    });
    expect(first.messages.map((message) => message.id)).toEqual([firstId]);
    expect(first.hasMore).toBe(true);
    const position: MessageHistoryPosition | undefined = first.lastPosition;
    expect(position?.deliveryCursor).toBe(first.messages[0]?.deliveryCursor);
    const next = await group.messageHistoryPage(
      { limit: 1, kind: "application" },
      undefined,
      position,
    );
    expect(next.messages.map((message) => message.id)).toEqual([secondId]);
    expect(next.hasMore).toBe(false);
  } finally {
    await client.end();
  }
});
