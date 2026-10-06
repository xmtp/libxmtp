import { setTimeout as sleep } from "node:timers/promises";

import { createRegisteredClient, createSigner } from "@test/helpers";
import { createRecoveryProxy } from "@test/recoveryProxy";
import {
  type ConversationStream,
  type Client,
  type ConnectionState,
} from "@xmtp/node-sdk";
import { expect, it } from "vitest";

const DELIVERY_WAIT = { timeout: 60_000, interval: 100 };
// Production wire silence can take three 30-second keepalive intervals.
const RECOVERY_WAIT = { timeout: 180_000, interval: 100 };

// verifies: PROC-021
it("recovers missed conversation notifications after an inbound blackhole", async () => {
  const proxy = await createRecoveryProxy();
  const clients: Client[] = [];
  const received: string[] = [];
  const connectionStates: ConnectionState[] = [];
  const errors: unknown[] = [];
  const closed: unknown[] = [];
  let stream: ConversationStream | undefined;
  let consumption: Promise<void> | undefined;
  try {
    const sender = await createRegisteredClient(createSigner().signer, {
      deviceSync: false,
    });
    clients.push(sender);
    const receiver = await createRegisteredClient(createSigner().signer, {
      backend: { url: proxy.url },
      deviceSync: false,
    });
    clients.push(receiver);
    stream = receiver.conversations.stream({
      conversationKind: "group",
      onConnectionStateChange: (_previous, current) => {
        connectionStates.push(current);
      },
      onClose: (reason) => {
        closed.push(reason);
        if (reason.kind === "failed") errors.push(reason.error);
      },
    });
    await stream.ready();
    consumption = stream
      .onValue((conversation) => {
        received.push(conversation.id);
      })
      .catch((error: unknown) => {
        errors.push(error);
      });

    const first = await sender.conversations.createGroup([receiver.inboxId]);
    await expect.poll(() => received, DELIVERY_WAIT).toEqual([first.id]);
    await expect
      .poll(() => connectionStates.at(-1), DELIVERY_WAIT)
      .toBe("connected");
    const beforeFault = connectionStates.length;

    proxy.inject("blackhole-inbound");
    const missed = await sender.conversations.createGroup([receiver.inboxId]);
    // Keep the link silent across a full advertised keepalive interval.
    await sleep(30_000);
    expect(received).toEqual([first.id]);
    await expect
      .poll(
        () =>
          connectionStates
            .slice(beforeFault)
            .some((state) => state !== "connected"),
        RECOVERY_WAIT,
      )
      .toBe(true);
    expect(errors).toEqual([]);
    expect(closed).toEqual([]);

    // Recovery uses the same stream. Do not sync or reopen the receiver.
    proxy.restore();
    await expect
      .poll(() => received, RECOVERY_WAIT)
      .toEqual([first.id, missed.id]);
    await expect
      .poll(() => connectionStates.at(-1), RECOVERY_WAIT)
      .toBe("connected");
    const after = await sender.conversations.createGroup([receiver.inboxId]);
    await expect
      .poll(() => received, DELIVERY_WAIT)
      .toEqual([first.id, missed.id, after.id]);
    expect(errors).toEqual([]);
    expect(closed).toEqual([]);
  } finally {
    proxy.restore();
    await stream?.end();
    await consumption;
    await Promise.allSettled(clients.map((client) => client.end()));
    await proxy.close();
  }
}, 660_000);
