import {
  type ConnectionState,
  type Message,
  type StreamCloseReason,
} from "@xmtp/browser-sdk";
import { expect, test } from "vitest";
import { commands } from "vitest/browser";

import { backend, create } from "./helpers";

const RECOVERY = { timeout: 90_000, interval: 100 };

function text(message: Message): string | undefined {
  return message.content.kind === "text" ? message.content.value : undefined;
}

// The receiver reaches the backend through a proxy that resets every
// connection and refuses new ones. The open stream must stay open and
// deliver a message sent after the proxy accepts connections again.
test("a stream stays open through a dropped connection and delivers after it recovers", async () => {
  const proxy = await commands.startRecoveryProxy();
  try {
    const sender = await create();
    const receiver = await create(undefined, {
      backend: { ...backend, url: proxy.url },
    });
    const group = await sender.conversations.createGroup([receiver.inboxId]);
    await receiver.conversations.sync();

    const states: ConnectionState[] = [];
    const reasons: StreamCloseReason[] = [];
    const received: string[] = [];
    const stream = receiver.conversations.streamAllMessages({
      onConnectionStateChange: (_previous, current) => states.push(current),
      onClose: (reason) => reasons.push(reason),
    });
    await stream.ready();
    const consumption = stream.onValue((message) => {
      const value = text(message);
      if (value) received.push(value);
    });

    await group.sendText("before the drop");
    await expect.poll(() => received, RECOVERY).toContain("before the drop");

    // Only states after the drop count; the first state can be "connecting".
    const dropped = states.length;
    await commands.dropRecoveryProxy(proxy.id);
    await expect
      .poll(
        () => states.slice(dropped).some((state) => state !== "connected"),
        RECOVERY,
      )
      .toBe(true);
    await commands.restoreRecoveryProxy(proxy.id);
    await group.sendText("after the drop");
    await expect.poll(() => received, RECOVERY).toContain("after the drop");
    await expect.poll(() => states.at(-1), RECOVERY).toBe("connected");
    expect(reasons).toStrictEqual([]);

    await stream.end();
    await consumption;
    expect(reasons).toStrictEqual([{ kind: "closed" }]);
    await receiver.end();
  } finally {
    await commands.closeRecoveryProxy(proxy.id);
  }
}, 240_000);
