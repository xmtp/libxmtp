import { expect, it } from "vitest";

import {
  Client,
  type MessageRecoveryPage,
  type MessageRecoveryPosition,
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
    const firstId = await group.sendText("first Agent pending message", {
      optimistic: true,
    });
    const secondId = await group.sendText("second Agent pending message", {
      optimistic: true,
    });
    const first: MessageRecoveryPage = await group.messageRecoveryPage({
      limit: 1,
      kind: "application",
    });
    expect(first.messages.map((message) => message.id)).toEqual([firstId]);
    expect(first.hasMore).toBe(true);
    const position: MessageRecoveryPosition | undefined = first.lastPosition;
    expect(position?.sentAt).toEqual(first.messages[0]?.sentAt);
    expect(position?.messageCursor).toBeTypeOf("string");
    const next = await group.messageRecoveryPage(
      { limit: 1, kind: "application" },
      undefined,
      position,
    );
    expect(next.messages.map((message) => message.id)).toEqual([secondId]);
    expect(next.hasMore).toBe(false);
    await group.publishMessage(firstId);
    const fresh = await group.sendText("after publication", {
      optimistic: true,
    });
    const resumed = await group.messageRecoveryPage(
      undefined,
      undefined,
      position,
    );
    expect(resumed.messages.map((message) => message.id)).toEqual([fresh]);
  } finally {
    await client.end();
  }
});
