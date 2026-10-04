import { createRegisteredClient, createSigner } from "@test/helpers";
import { createRecoveryBackend } from "@test/recoveryBackend";
import {
  MessageStream,
  XmtpError,
  type Client,
  type Message,
} from "@xmtp/node-sdk";
import { describe, expect, it } from "vitest";

const WAIT = { timeout: 60_000, interval: 100 };
const RUN = process.env.XMTP_RECOVERY_FOCUSED === "1";

async function receiverStream(
  receiver: Client,
  received: Array<{ id: string; body: string }>,
  sentReplies: Map<string, string>,
  failures: Error[],
) {
  const stream = MessageStream.open(
    receiver,
    {},
    {
      onClose: (reason) => {
        if (reason.kind === "failed") failures.push(reason.error as Error);
      },
    },
  );
  await stream.ready();
  void stream
    .onValue(async (message: Message) => {
      if (message.content.kind !== "text") return;
      const body = message.content.value;
      if (!body.startsWith("focused-request:")) return;
      received.push({ id: message.id, body });
      const group = await receiver.conversations.getById(
        message.conversationId,
      );
      if (!group) throw new Error("The received group is missing");
      const replyBody = body.replace("focused-request:", "focused-reply:");
      sentReplies.set(replyBody, await group.sendText(replyBody));
    })
    .catch(() => {});
  return stream;
}

describe.runIf(RUN)("focused graceful recovery", () => {
  it.each([
    ["within budget", 1],
    ["over budget", 8],
  ] as const)(
    "recovers %s with %i groups",
    async (caseName, count) => {
      const backend = await createRecoveryBackend();
      const clients: Client[] = [];
      const streams: MessageStream[] = [];
      const received: Array<{ id: string; body: string }> = [];
      const replies: Array<{ id: string; body: string }> = [];
      const sentReplies = new Map<string, string>();
      const failures: Error[] = [];
      try {
        const sender = await createRegisteredClient(createSigner().signer, {
          deviceSync: false,
        });
        clients.push(sender);
        const receiver = await createRegisteredClient(createSigner().signer, {
          backend: { url: backend.url },
          deviceSync: false,
        });
        clients.push(receiver);
        const groups = [];
        for (let index = 0; index < count; index++)
          groups.push(
            await sender.conversations.createGroup([receiver.inboxId]),
          );
        await receiver.conversations.sync();
        const peerStream = MessageStream.open(sender);
        await peerStream.ready();
        void peerStream
          .onValue((message) => {
            if (
              message.content.kind === "text" &&
              message.content.value.startsWith("focused-reply:")
            )
              replies.push({ id: message.id, body: message.content.value });
          })
          .catch(() => {});
        streams.push(peerStream);
        let stream = await receiverStream(
          receiver,
          received,
          sentReplies,
          failures,
        );
        streams.push(stream);
        const sent = new Map<string, string>();
        for (let index = 0; index < count; index++) {
          const body = `focused-request:${index}:baseline`;
          sent.set(body, await groups[index].sendText(body));
        }
        await expect.poll(() => received.length, WAIT).toBe(count);
        await expect.poll(() => replies.length, WAIT).toBe(count);
        await backend.stopGracefully();
        for (let index = 0; index < count; index++) {
          const body = `focused-request:${index}:outage`;
          sent.set(body, await groups[index].sendText(body));
        }
        if (caseName === "over budget") {
          await expect.poll(() => failures.length, WAIT).toBe(1);
          const failure = failures[0];
          expect(failure).toBeInstanceOf(XmtpError.RecoveryExhausted);
          expect(failure).toMatchObject({
            details: { code: "RecoveryExhausted", category: "stream" },
          });
          await expect(stream.next()).rejects.toBe(failure);
        }
        await backend.start();
        if (caseName === "over budget") {
          stream = await receiverStream(
            receiver,
            received,
            sentReplies,
            failures,
          );
          streams.push(stream);
        }
        await expect.poll(() => received.length, WAIT).toBe(count * 2);
        await expect.poll(() => replies.length, WAIT).toBe(count * 2);
        await expect.poll(() => sentReplies.size, WAIT).toBe(count * 2);
        for (const [body, id] of sent) {
          expect(received.filter((message) => message.body === body)).toEqual([
            { id, body },
          ]);
        }
        for (const [body, id] of sentReplies) {
          expect(replies.filter((message) => message.body === body)).toEqual([
            { id, body },
          ]);
        }
        const followUp = `focused-request:0:after`;
        const followUpId = await groups[0].sendText(followUp);
        await expect.poll(() => received.length, WAIT).toBe(count * 2 + 1);
        await expect.poll(() => replies.length, WAIT).toBe(count * 2 + 1);
        await expect.poll(() => sentReplies.size, WAIT).toBe(count * 2 + 1);
        expect(received.filter((message) => message.body === followUp)).toEqual(
          [{ id: followUpId, body: followUp }],
        );
        const followUpReply = followUp.replace(
          "focused-request:",
          "focused-reply:",
        );
        expect(
          replies.filter((message) => message.body === followUpReply),
        ).toEqual([
          { id: sentReplies.get(followUpReply), body: followUpReply },
        ]);
        expect(failures).toHaveLength(caseName === "over budget" ? 1 : 0);
        await expect(stream.ready()).resolves.toBeUndefined();
      } finally {
        await Promise.allSettled(streams.map((stream) => stream.end()));
        await Promise.allSettled(clients.map((client) => client.end()));
        await backend.close();
      }
    },
    180_000,
  );
});
