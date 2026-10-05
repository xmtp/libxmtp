import { expect, test } from "vitest";

import { create } from "./helpers";

test("cold catch-up reads an owed group and message once without a separate sync", async () => {
  const sender = await create();
  const peer = await create();
  const group = await sender.conversations.createGroup([peer.inboxId]);
  const id = await group.sendText("missed while away", { shouldPush: true });
  expect(await peer.conversations.list()).toEqual([]);

  const summary = await peer.catchUpToLive(undefined);
  expect(summary.completed).toBe(true);
  expect(summary.failed).toBe(0n);
  expect(summary.conversations).toBeGreaterThanOrEqual(1n);
  expect(summary.messages).toBeGreaterThanOrEqual(1n);
  const received = await peer.conversations.getById(group.id);
  if (!received) throw new Error("Catch-up did not retain the owed group");
  const message = (await received.messages()).find((item) => item.id === id);
  expect(message?.content).toEqual({
    kind: "text",
    value: "missed while away",
  });

  const again = await peer.catchUpToLive(undefined);
  expect(again.completed).toBe(true);
  expect(again.failed).toBe(0n);
  expect(again.messages).toBe(0n);
  expect(again.conversations).toBe(0n);
  expect((await peer.conversations.list()).map((item) => item.id)).toEqual([
    group.id,
  ]);
});
