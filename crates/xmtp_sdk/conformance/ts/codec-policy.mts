// The typed codec send policy through the public Message.reply (Ref Public
// surface, Host codecs; P10). Codec steps run before the send; a failed step
// is CodecEncodeFailed and makes no publish attempt.
import assert from "node:assert/strict";

import * as sdk from "../../../../target/sdk-conformance/typescript-napi/index.ts";

type Note = { readonly text: string };

const noteType: sdk.ContentTypeId = {
  authorityId: "example.org",
  typeId: "note",
  versionMajor: 1,
  versionMinor: 0,
};

function throwing(step: string): never {
  throw new Error(`${step} must not run`);
}

// A codec whose steps are replaced per case.
function noteCodec(
  steps: Partial<sdk.ContentCodec<Note>> = {},
): sdk.ContentCodec<Note> {
  return {
    type: noteType,
    encode: (value) => ({
      type: noteType,
      parameters: new Map(),
      content: new TextEncoder().encode(value.text),
    }),
    decode: (encoded) => ({ text: new TextDecoder().decode(encoded.content) }),
    ...steps,
  };
}

function isCodecEncodeFailed(error: unknown): boolean {
  return (
    error instanceof sdk.XmtpError.CodecEncodeFailed &&
    error.details.code === "CodecEncodeFailed" &&
    error.details.category === "callback" &&
    error.details.retryable === false
  );
}

// The nested envelope of a stored reply.
async function nestedEnvelope(
  group: sdk.Group,
  id: sdk.MessageId,
): Promise<sdk.EncodedContent> {
  const reply = (await group.messages()).find((message) => message.id === id);
  assert.ok(reply?.content.kind === "reply", "the reply was not stored");
  const body = reply.content.body;
  assert.ok(body.kind === "custom" || body.kind === "unknown");
  return body.encoded;
}

async function replyHooks(
  group: sdk.Group,
  parent: sdk.Message,
): Promise<void> {
  // The fallback hook fills an envelope that has none.
  const filled = await parent.reply(
    noteCodec({
      fallback: (value) => `a note: ${value.text}`,
      shouldPush: () => throwing("shouldPush for a reply"),
    }),
    { text: "filled" },
  );
  assert.equal(
    (await nestedEnvelope(group, filled)).fallback,
    "a note: filled",
  );

  // An envelope with a fallback keeps it; the hook is not called.
  const kept = await parent.reply(
    noteCodec({
      encode: (value) => ({
        type: noteType,
        parameters: new Map(),
        fallback: "own fallback",
        content: new TextEncoder().encode(value.text),
      }),
      fallback: () => throwing("fallback with an envelope fallback"),
    }),
    { text: "kept" },
  );
  assert.equal((await nestedEnvelope(group, kept)).fallback, "own fallback");

  // A class codec's hooks run on the codec, so they can use `this`.
  class LabelledNotes implements sdk.ContentCodec<Note> {
    readonly type = noteType;
    readonly label = "labelled";
    encode(value: Note): sdk.EncodedContent {
      return noteCodec().encode(value);
    }
    decode(encoded: sdk.EncodedContent): Note {
      return noteCodec().decode(encoded);
    }
    fallback(value: Note): string {
      return `${this.label} ${value.text}`;
    }
  }
  const labelled = await parent.reply(new LabelledNotes(), { text: "note" });
  assert.equal(
    (await nestedEnvelope(group, labelled)).fallback,
    "labelled note",
  );

  // No hook: no fallback.
  const plain = await parent.reply(noteCodec(), { text: "plain" });
  assert.equal((await nestedEnvelope(group, plain)).fallback, undefined);
}

// A failed or invalid step fails the reply before any publish attempt.
async function replyFailures(
  group: sdk.Group,
  parent: sdk.Message,
): Promise<void> {
  process.on("unhandledRejection", recordUnhandled);
  const before = (await group.messages()).length;
  for (const [step, codec] of [
    ["encode", noteCodec({ encode: () => throwing("encode") })],
    [
      "encode result",
      noteCodec({
        encode: () => Promise.resolve() as unknown as sdk.EncodedContent,
      }),
    ],
    ["fallback", noteCodec({ fallback: () => throwing("fallback") })],
    ["fallback result", noteCodec({ fallback: () => 7 as unknown as string })],
    [
      "async encode",
      noteCodec({
        encode: (() =>
          Promise.reject(
            new Error("async encode"),
          )) as unknown as () => sdk.EncodedContent,
      }),
    ],
    [
      "async fallback",
      noteCodec({
        fallback: (() =>
          Promise.reject(
            new Error("async fallback"),
          )) as unknown as () => string,
      }),
    ],
    [
      "envelope fallback",
      noteCodec({
        encode: (value) =>
          ({
            type: noteType,
            fallback: 7,
            content: new TextEncoder().encode(value.text),
          }) as unknown as sdk.EncodedContent,
      }),
    ],
    [
      "envelope of another type",
      noteCodec({
        encode: (value) => ({
          type: { ...noteType, typeId: "other" },
          content: new TextEncoder().encode(value.text),
        }),
      }),
    ],
    [
      "envelope type",
      noteCodec({
        encode: (value) =>
          ({
            type: { authorityId: "example.org", typeId: "note" },
            content: new TextEncoder().encode(value.text),
          }) as unknown as sdk.EncodedContent,
      }),
    ],
  ] as const) {
    await assert.rejects(
      parent.reply(codec, { text: step }),
      isCodecEncodeFailed,
      `${step} did not fail with CodecEncodeFailed`,
    );
  }
  assert.equal(
    (await group.messages()).length,
    before,
    "a failed codec step made a publish attempt",
  );
  // A codec check that throws (a Proxy trap) is a codec failure on reply too.
  await assert.rejects(
    parent.reply(
      new Proxy(noteCodec(), {
        has() {
          throw new Error("has trap");
        },
      }),
      { text: "trap" },
    ),
    isCodecEncodeFailed,
    "a throwing codec check on reply did not fail with CodecEncodeFailed",
  );
  // A rejected async step is handled, not left to end the process.
  await new Promise((resolve) => setTimeout(resolve, 20));
  assert.deepEqual(
    unhandled,
    [],
    "an async codec step rejection was unhandled",
  );
  process.off("unhandledRejection", recordUnhandled);
}

const unhandled: unknown[] = [];
function recordUnhandled(reason: unknown): void {
  unhandled.push(reason);
}

async function stored(
  group: sdk.Group,
  id: sdk.MessageId,
): Promise<sdk.Message> {
  const message = (await group.messages()).find((item) => item.id === id);
  assert.ok(message, "the message was not stored");
  return message;
}

async function sendHooks(
  group: sdk.Group,
  receiver: sdk.Client,
): Promise<void> {
  // A typed send fills the fallback and keeps the codec's value type. The
  // sender has no registered codec for the type, so the content is unknown.
  const codec = noteCodec({ fallback: (value) => `a note: ${value.text}` });
  const sentId = await group.send(codec, { text: "typed send" });
  const sent = (await stored(group, sentId)).content;
  assert.ok(sent.kind === "unknown", `typed send content is ${sent.kind}`);
  assert.equal(sent.encoded.fallback, "a note: typed send");
  // A gzip send is readable by the receiver. Node cannot see whether the
  // stored envelope is compressed (its raw bytes are not an EncodedContent), so
  // this checks only the round trip. The vitest policy test checks that the
  // compression option reaches the send, and the Rust
  // message_actions_use_ids_and_compression_is_opt_in test checks the stored
  // envelope.
  const gzipId = await group.send(
    codec,
    { text: "gzip" },
    { compression: "gzip" },
  );

  // prepareMessage takes the same codec form and stores an unpublished item.
  const preparedId = await group.prepareMessage(codec, { text: "prepared" });
  assert.equal((await stored(group, preparedId)).deliveryStatus, "unpublished");
  await group.publishMessage(preparedId);

  // A receiver without the codec keeps the envelope and its fallback.
  await receiver.conversations.syncAll(undefined);
  const received = await receiver.conversations.getMessageById(sentId);
  assert.ok(received, "the receiver did not get the typed send");
  assert.ok(
    received.content.kind === "unknown",
    "missing codec is not unknown",
  );
  assert.equal(received.content.encoded.fallback, "a note: typed send");
  const gzip = await receiver.conversations.getMessageById(gzipId);
  assert.ok(gzip?.content.kind === "unknown", "the gzip send did not arrive");
  assert.equal(new TextDecoder().decode(gzip.content.encoded.content), "gzip");
  assert.equal(gzip.content.encoded.fallback, "a note: gzip");
}

// A failed codec step makes no send attempt, on send and prepareMessage.
async function sendFailures(group: sdk.Group): Promise<void> {
  const before = (await group.messages()).length;
  const failing = noteCodec({ shouldPush: () => throwing("shouldPush") });
  await assert.rejects(group.send(failing, { text: "x" }), isCodecEncodeFailed);
  await assert.rejects(
    group.prepareMessage(failing, { text: "x" }),
    isCodecEncodeFailed,
  );
  assert.equal((await group.messages()).length, before);
  // An explicit shouldPush skips the hook.
  await group.send(failing, { text: "explicit" }, { shouldPush: false });
}

/**
 * custom_codec_policy_and_isolation: typed sends, prepares, and replies apply
 * the codec's fallback and push hooks, and a client without the codec keeps
 * the envelope and its fallback.
 */
// verifies: CTYPE-017, CTYPE-021
export async function customCodecPolicyAndIsolation(
  group: sdk.Group,
  parent: sdk.Message,
  receiver: sdk.Client,
): Promise<void> {
  await replyHooks(group, parent);
  await sendHooks(group, receiver);
}

/**
 * codec_policy_failure_never_publishes: a failed or invalid encode, fallback,
 * or shouldPush step is CodecEncodeFailed with no publish attempt, and a
 * skipped hook is not called.
 */
// verifies: CTYPE-007
export async function codecPolicyFailureNeverPublishes(
  group: sdk.Group,
  parent: sdk.Message,
): Promise<void> {
  await replyFailures(group, parent);
  await sendFailures(group);
  await standardSubclassHooks(group);
}

// Standard subclasses retain custom hooks. The send path encodes only once.
async function standardSubclassHooks(group: sdk.Group): Promise<void> {
  class TextWithFallback extends sdk.TextCodec {
    encodes = 0;
    override encode(value: string): sdk.EncodedContent {
      this.encodes++;
      return { ...super.encode(value), fallback: undefined };
    }
    override fallback(value: string): string {
      return `custom text: ${value}`;
    }
  }
  const custom = new TextWithFallback();
  const id = await group.send(custom, "subclass", { shouldPush: false });
  assert.equal(custom.encodes, 1, "standard subclass encoded more than once");
  assert.equal((await stored(group, id)).fallback, "custom text: subclass");

  class BrokenFallback extends TextWithFallback {
    override fallback(): string {
      throw new Error("private fallback error");
    }
  }
  const before = (await group.messages()).length;
  const broken = new BrokenFallback();
  await assert.rejects(
    group.send(broken, "rejected", { shouldPush: false }),
    isCodecEncodeFailed,
  );
  assert.equal(broken.encodes, 1, "throwing subclass encoded more than once");
  assert.equal(
    (await group.messages()).length,
    before,
    "throwing fallback published a message",
  );
}
