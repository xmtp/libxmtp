import {
  Client,
  Group,
  XmtpError,
  type ContentCodec,
  type EncodedContent,
  type Message,
  type MessageId,
  type Signer,
} from "xmtp-sdk";

import { ReadingCodec, type Reading } from "./index.js";

function check(value: unknown, message: string): asserts value {
  if (!value) throw new Error(message);
}

function equal(actual: unknown, expected: unknown, message: string): void {
  check(JSON.stringify(actual) === JSON.stringify(expected), message);
}

function sameEnvelope(
  actual: EncodedContent | undefined,
  expected: EncodedContent,
): void {
  check(actual !== undefined, "received envelope is absent");
  for (const field of [
    "authorityId",
    "typeId",
    "versionMajor",
    "versionMinor",
  ] as const)
    equal(
      actual.type[field],
      expected.type[field],
      `content type ${field} changed`,
    );
  equal(
    [...(actual.parameters ?? [])].sort(([left], [right]) =>
      left.localeCompare(right),
    ),
    [...(expected.parameters ?? [])].sort(([left], [right]) =>
      left.localeCompare(right),
    ),
    "parameters changed",
  );
  equal(actual.fallback, expected.fallback, "fallback changed");
  equal([...actual.content], [...expected.content], "content bytes changed");
}

async function find(group: Group, id: MessageId): Promise<Message> {
  const message = (await group.messages()).find((item) => item.id === id);
  check(message !== undefined, "sent message is absent");
  return message;
}

function codecWith(
  codec: ReadingCodec,
  steps: Partial<ContentCodec<Reading>>,
): ContentCodec<Reading> {
  return {
    type: codec.type,
    encode: codec.encode,
    decode: codec.decode,
    fallback: codec.fallback,
    shouldPush: codec.shouldPush,
    ...steps,
  };
}

// verifies: CTYPE-001, CTYPE-007, CTYPE-017
// This package imports only the SDK root. Real clients send, receive and reply.
export async function exercise(
  signers: readonly [Signer, Signer, Signer],
  backendUrl: string,
): Promise<{
  readonly checks: string[];
  readonly conversationId: string;
  readonly rawEnvelopes: number[][];
}> {
  const senderCodec = new ReadingCodec(7);
  const receiverCodec = new ReadingCodec(0);
  const options = {
    backend: { url: backendUrl },
    storage: { location: "inMemory" as const },
    deviceSync: false,
  };
  const clients: Client[] = [];
  try {
    const sender = await Client.create(signers[0], {
      ...options,
      codecs: [senderCodec],
    });
    clients.push(sender);
    const receiver = await Client.create(signers[1], {
      ...options,
      codecs: [receiverCodec],
    });
    clients.push(receiver);
    const unknownReceiver = await Client.create(signers[2], options);
    clients.push(unknownReceiver);
    const group = await sender.conversations.createGroup([
      receiver.inboxId,
      unknownReceiver.inboxId,
    ]);
    const value: Reading = { text: "warm °C\n雪\u0000", revision: 9001 };
    const encoded = senderCodec.encode(value);
    equal(
      senderCodec.decode(encoded),
      value,
      "standalone codec round trip failed",
    );
    const sent = await group.send(senderCodec, value);
    const parent = await find(group, sent);
    check(
      parent.content.kind === "custom",
      "sender registry did not decode custom content",
    );
    equal(parent.content.value, value, "sender decoded another value");
    const expected = {
      ...encoded,
      fallback: `reading ${value.revision}: ${value.text}`,
    };
    sameEnvelope(parent.encoded, expected);
    equal(
      senderCodec.fallbackCalls,
      1,
      "send did not call the fallback hook once",
    );
    equal(
      senderCodec.pushCalls,
      1,
      "send did not call the custom push hook once",
    );

    for (const client of [receiver, unknownReceiver])
      await client.conversations.sync();
    for (const [client, kind] of [
      [receiver, "custom"],
      [unknownReceiver, "unknown"],
    ] as const) {
      const receivedGroup = await client.conversations.getById(group.id);
      check(receivedGroup instanceof Group, "receiver group is absent");
      await receivedGroup.sync();
      const message = await find(receivedGroup, sent);
      equal(message.content.kind, kind, "a registry escaped its client");
      sameEnvelope(message.encoded, expected);
      if (message.content.kind === "custom") {
        equal(
          message.content.value,
          value,
          "minor version changed codec matching",
        );
        equal(
          receiverCodec.decode(message.content.encoded),
          value,
          "typed decode failed",
        );
      }
    }

    // An empty encoded fallback is present. Both skipped hooks throw if called.
    const keptCodec = codecWith(senderCodec, {
      encode: new ReadingCodec(7, "").encode,
      fallback: () => {
        throw new Error("encoded fallback must win");
      },
      shouldPush: () => {
        throw new Error("explicit false must win");
      },
    });
    const kept = await group.send(keptCodec, value, { shouldPush: false });
    const keptMessage = await find(group, kept);
    sameEnvelope(keptMessage.encoded, {
      ...encoded,
      fallback: "",
    });

    // A reply uses the nested fallback and the outer catalogue push policy.
    const replyCodec = codecWith(senderCodec, {
      shouldPush: () => {
        throw new Error("nested reply push must not run");
      },
    });
    const replyValue: Reading = { text: "nested reply", revision: 2 };
    const replied = await parent.reply(replyCodec, replyValue);
    const reply = await find(group, replied);
    check(reply.content.kind === "reply", "reply lost its outer type");
    equal(reply.content.referenceId, sent, "reply reference changed");
    check(
      reply.content.body.kind === "custom",
      "nested custom value was not decoded",
    );
    equal(reply.content.body.value, replyValue, "reply decoded another value");
    sameEnvelope(reply.content.body.encoded, {
      ...senderCodec.encode(replyValue),
      fallback: "reading 2: nested reply",
    });
    equal(senderCodec.pushCalls, 1, "reply called the custom push hook");
    for (const [client, kind] of [
      [receiver, "custom"],
      [unknownReceiver, "unknown"],
    ] as const) {
      const receivedGroup = await client.conversations.getById(group.id);
      check(receivedGroup instanceof Group, "reply receiver group is absent");
      await receivedGroup.sync();
      const receivedReply = await find(receivedGroup, replied);
      check(
        receivedReply.content.kind === "reply",
        "received reply lost its outer type",
      );
      equal(
        receivedReply.content.referenceId,
        sent,
        "received reply reference changed",
      );
      const body = receivedReply.content.body;
      equal(body.kind, kind, "nested registry escaped its client");
      check(
        body.kind === "custom" || body.kind === "unknown",
        "nested type changed",
      );
      sameEnvelope(body.encoded, {
        ...senderCodec.encode(replyValue),
        fallback: "reading 2: nested reply",
      });
      if (body.kind === "custom")
        equal(body.value, replyValue, "received reply decoded another value");
    }

    const silentReply = await find(
      group,
      await parent.reply(replyCodec, replyValue, { shouldPush: false }),
    );
    check(
      silentReply.content.kind === "reply",
      "silent reply lost its outer type",
    );
    check(
      silentReply.content.body.kind === "custom",
      "silent reply lost its nested codec",
    );

    const before = (await group.messages()).length;
    const failures: ContentCodec<Reading>[] = [
      codecWith(senderCodec, {
        encode: () => {
          throw new Error("codec author encode failure");
        },
      }),
      codecWith(senderCodec, {
        fallback: () => {
          throw new Error("codec author fallback failure");
        },
      }),
      codecWith(senderCodec, {
        shouldPush: () => {
          throw new Error("codec author push failure");
        },
      }),
    ];
    for (const codec of failures) {
      let failed = false;
      try {
        await group.send(codec, value);
      } catch (error) {
        failed = error instanceof XmtpError.CodecEncodeFailed;
      }
      check(failed, "codec failure lost its public error code");
    }
    equal(
      (await group.messages()).length,
      before,
      "failed codec step stored a message",
    );
    return {
      checks: [
        "standalone encode/decode",
        "public send and minor-version receive",
        "exact envelope fields",
        "per-client registry",
        "fallback and skipped push hooks",
        "public nested reply",
        "failed codec steps before send",
      ],
      conversationId: group.id,
      rawEnvelopes: [
        parent.rawBytes,
        keptMessage.rawBytes,
        reply.rawBytes,
        silentReply.rawBytes,
        reply.content.body.rawBytes,
        silentReply.content.body.rawBytes,
      ].map((bytes) => [...bytes]),
    };
  } finally {
    for (const client of clients.reverse()) await client.end();
  }
}
