import { expect, test } from "vitest";

import { create, signer } from "./helpers";

test("both peer installations keep membership history and exclude messages sent while removed", async () => {
  const creator = await create();
  const owner = signer();
  const peer = await create(owner);
  const secondInstallation = await create(owner);
  const observer = await create();
  expect(secondInstallation.inboxId).toBe(peer.inboxId);
  expect(secondInstallation.installationId).not.toBe(peer.installationId);
  const group = await creator.conversations.createGroup([
    peer.inboxId,
    observer.inboxId,
  ]);

  // Receive the first Welcome before the member leaves and joins again.
  for (const client of [peer, secondInstallation, observer]) {
    await client.conversations.sync();
    const received = await client.conversations.getById(group.id);
    if (!received) throw new Error("Initial peer group missing");
    const initial = await received.messages();
    expect(initial).toHaveLength(1);
    expect(initial[0].kind).toBe("membershipChange");
    expect(initial[0].content.kind).toBe("groupUpdated");
    expect(initial[0].conversationId).toBe(group.id);
  }

  const beforeRemoval = await group.sendText("before removal", {
    shouldPush: false,
  });
  await group.removeMembers([peer.inboxId]);
  const whileRemoved = await group.sendText("while removed", {
    shouldPush: false,
  });
  await group.addMembers([peer.inboxId]);
  const afterJoin = await group.sendText("after join", { shouldPush: false });
  const allKinds = [
    "groupUpdated",
    "text",
    "groupUpdated",
    "text",
    "groupUpdated",
    "text",
  ];
  expect(
    (await group.messages({ direction: "ascending" })).map(
      (message) => message.content.kind,
    ),
  ).toEqual(allKinds);

  for (const client of [peer, secondInstallation, observer]) {
    await client.conversations.sync();
    const received = await client.conversations.getById(group.id);
    if (!received) throw new Error("Peer group missing after join");
    await received.sync();
    const messages = await received.messages({ direction: "ascending" });
    for (const message of messages)
      expect(message.conversationId).toBe(group.id);
    if (client === observer) {
      expect(messages.map((message) => message.content.kind)).toEqual(allKinds);
      expect(
        messages
          .filter((message) => message.kind === "application")
          .map((message) => message.id),
      ).toEqual([beforeRemoval, whileRemoved, afterJoin]);
    } else {
      expect(messages.map((message) => message.content.kind)).toEqual([
        "groupUpdated",
        "text",
        "groupUpdated",
        "groupUpdated",
        "text",
      ]);
      const application = messages.filter(
        (message) => message.kind === "application",
      );
      expect(application.map((message) => message.id)).toEqual([
        beforeRemoval,
        afterJoin,
      ]);
      expect(application.map((message) => message.content)).toEqual([
        { kind: "text", value: "before removal" },
        { kind: "text", value: "after join" },
      ]);
      expect(messages.map((message) => message.id)).not.toContain(whileRemoved);
    }
  }
});
