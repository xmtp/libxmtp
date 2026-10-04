import { createRegisteredClient, createSigner } from "@test/helpers";
import { MessageStream } from "@xmtp/node-sdk";
import { expect, it, vi } from "vitest";

const WAIT = { timeout: 30_000, interval: 100 };

it("defaults all-message delivery to allowed and unknown consent", async () => {
  const sender = await createRegisteredClient(createSigner().signer);
  const receiver = await createRegisteredClient(createSigner().signer);
  let stream: MessageStream | undefined;
  let read: Promise<void> | undefined;
  try {
    const allowed = await sender.conversations.createGroup([receiver.inboxId]);
    const unknown = await sender.conversations.createGroup([receiver.inboxId]);
    const denied = await sender.conversations.createGroup([receiver.inboxId]);
    await receiver.conversations.syncAll(undefined);
    await receiver.preferences.setConsentStates([
      {
        entity: { kind: "conversation", conversationId: allowed.id },
        state: "allowed",
      },
      {
        entity: { kind: "conversation", conversationId: unknown.id },
        state: "unknown",
      },
      {
        entity: { kind: "conversation", conversationId: denied.id },
        state: "denied",
      },
    ]);
    const deniedId = await denied.sendText("denied");
    const allowedId = await allowed.sendText("allowed");
    const unknownId = await unknown.sendText("unknown");
    await receiver.conversations.syncAll(undefined);

    stream = MessageStream.open(receiver);
    await stream.ready();
    const seen: string[] = [];
    read = stream.onValue((message) => {
      seen.push(message.id);
    });
    const barrier = await allowed.sendText("after stored rows");
    await vi.waitFor(() => {
      expect(seen).toContain(allowedId);
      expect(seen).toContain(unknownId);
      expect(seen).toContain(barrier);
    }, WAIT);
    expect(seen).not.toContain(deniedId);
  } finally {
    await stream?.end();
    await read;
    await receiver.end();
    await sender.end();
  }
});

it("delivers a new DM installation without a receiver sync", async () => {
  const receiver = await createRegisteredClient(createSigner().signer);
  const owner = createSigner().signer;
  const first = await createRegisteredClient(owner);
  const clients = [receiver, first];
  let stream: MessageStream | undefined;
  let read: Promise<void> | undefined;
  try {
    const firstDm = await first.conversations.createDm(receiver.inboxId);
    stream = MessageStream.open(receiver, { conversationKind: "dm" });
    await stream.ready();
    const seen: string[] = [];
    read = stream.onValue((message) => {
      seen.push(message.id);
    });
    const firstId = await firstDm.sendText("first installation");
    await vi.waitFor(() => expect(seen).toContain(firstId), WAIT);

    const second = await createRegisteredClient(owner);
    clients.push(second);
    expect(second.inboxId).toBe(first.inboxId);
    expect(second.installationId).not.toBe(first.installationId);
    const secondDm = await second.conversations.createDm(receiver.inboxId);
    const secondId = await secondDm.sendText("new installation");
    await vi.waitFor(() => expect(seen).toContain(secondId), WAIT);
    expect(seen.filter((id) => id === firstId)).toHaveLength(1);
    expect(seen.filter((id) => id === secondId)).toHaveLength(1);
    expect(
      (await receiver.conversations.getMessageById(secondId))?.content,
    ).toEqual({ kind: "text", value: "new installation" });
  } finally {
    await stream?.end();
    await read;
    await Promise.allSettled(clients.map((client) => client.end()));
  }
});
