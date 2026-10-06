// The generated package root projects the binding: it hides binding shapes,
// factories and tagged-union internals, and keeps record fields read-only.
// Each `@ts-expect-error` line must fail to compile. If the generator starts
// to expose one of these, `tsc` reports an unused directive.
import type * as Sdk from "@xmtp/node-sdk";
import type {
  BackendOptions,
  Client,
  ContentCodec,
  ContentTypeId,
  Conversation,
  ConversationId,
  DeleteMessageCodec,
  Dm,
  EncodedContent,
  ErrorDetails,
  LogLevel,
  LogSink,
  Message,
  MessageBody,
  MessageContent,
  PublicIdentity,
  ReactionV2Codec,
  ReplyCodec,
  Signer,
  StandardContent,
  Timestamp,
} from "@xmtp/node-sdk";
import { Group } from "@xmtp/node-sdk";

type Equal<Left, Right> =
  (<Value>() => Value extends Left ? 1 : 2) extends <
    Value,
  >() => Value extends Right ? 1 : 2
    ? true
    : false;
type Assert<Value extends true> = Value;

// @ts-expect-error The binding session type is not exported.
export type MainSession = Sdk.MainSession;
// @ts-expect-error Tagged-union tag enums are not exported.
export type StandardContentTags = Sdk.StandardContent_Tags;
// @ts-expect-error The internal log handoff is not exported.
export type LogSinkHandoff = typeof Sdk.sdkLogSinkHandoff;
// @ts-expect-error The queued log sink is not exported.
export type QueuedLogSink = typeof Sdk.setLogSinkQueued;

export function rejectBindingShapes(
  client: Client,
  group: Group,
  conversation: Conversation,
  content: MessageContent,
  identity: PublicIdentity,
): void {
  // @ts-expect-error A conversation ID is a string.
  const id: ConversationId = 42;
  // @ts-expect-error Decoded content is not encoded content.
  const encoded: EncodedContent = content;
  // @ts-expect-error A conversation is not a tagged binding union.
  const tag: unknown = conversation.tag;
  // @ts-expect-error Message content is not a tagged binding union.
  const inner: unknown = content.inner;
  // @ts-expect-error Objects come from the SDK, not from a constructor.
  const created = new Group();
  // @ts-expect-error There is no binding factory.
  const factory: unknown = Group.new;
  // @ts-expect-error `conversations` is a property, not a binding method.
  const conversations: unknown = client.conversations();
  // @ts-expect-error Objects do not expose their native handle.
  const handle: unknown = group.handle;
  void [id, encoded, tag, inner, created, factory, conversations, handle];

  // @ts-expect-error Record fields are read-only.
  identity.identifier = "changed";
  // @ts-expect-error Record fields are read-only.
  identity.kind = "passkey";
}

export const credentialWidth: BackendOptions = {
  url: "https://example.test",
  // @ts-expect-error Credential expiry keeps its uint64 bigint width.
  credentials: { value: "token", expiresAtSeconds: 1 },
};

// @ts-expect-error Content bytes are a Uint8Array.
export const contentBytes: EncodedContent["content"] = new ArrayBuffer(2);

// The asynchronous log sink receives projected record fields.
type LogRecordFields = Parameters<LogSink["log"]>[0];
export type LogSinkIsAsync = Assert<
  Equal<ReturnType<LogSink["log"]>, Promise<void>>
>;
export type LogRecordShape = Assert<
  Equal<
    LogRecordFields,
    {
      readonly level: LogLevel;
      readonly target: string;
      readonly message: string;
      readonly fields: Map<string, string>;
      readonly timestamp: Timestamp;
      readonly droppedRecords: bigint;
    }
  >
>;

// Public value fields keep their projected types.
export type IdentityKind = Assert<
  Equal<PublicIdentity["kind"], "ethereum" | "passkey">
>;
export type SignerResult = Assert<
  Equal<Awaited<ReturnType<Signer["identity"]>>, PublicIdentity>
>;
export type ReceivedBytes = Assert<Equal<Message["rawBytes"], Uint8Array>>;
export type ReceivedEnvelope = Assert<
  Equal<Message["encoded"], EncodedContent | undefined>
>;
export type ReceivedType = Assert<
  Equal<Message["contentType"], ContentTypeId | undefined>
>;
export type UnknownDetails = Assert<
  Equal<Extract<MessageContent, { kind: "unknown" }>["error"], ErrorDetails>
>;
export type CustomDetails = Assert<
  Equal<
    Extract<MessageBody, { kind: "custom" }>["error"],
    ErrorDetails | undefined
  >
>;

// Typed codecs keep their value type through the public send and reply
// helpers. A standard codec for one StandardContent variant takes only that
// variant.
type Point = { readonly x: number; readonly y: number };

declare const pointCodec: ContentCodec<Point>;
declare const textCodec: ContentCodec<string>;
declare const literalCodec: ContentCodec<"a" | "b">;
declare const anyText: string;
declare const reactions: ReactionV2Codec;
declare const replies: ReplyCodec;
declare const deletions: DeleteMessageCodec;

// verifies: CTYPE-017
export async function rejectWrongCodecValues(
  group: Group,
  dm: Dm,
  message: Message,
  reaction: Extract<StandardContent, { kind: "reaction" }>,
): Promise<void> {
  await group.send(reactions, reaction);
  await group.send(pointCodec, { x: 1, y: 2 });
  await message.reply(pointCodec, { x: 1, y: 2 });

  // @ts-expect-error A reaction codec does not take text content.
  await group.send(reactions, { kind: "text", value: "x" });
  // @ts-expect-error A reaction codec does not take text content.
  await dm.send(reactions, { kind: "text", value: "x" });
  // @ts-expect-error A reaction codec does not encode text content.
  reactions.encode({ kind: "text", value: "x" });
  // @ts-expect-error A reply codec does not take text content.
  await group.send(replies, { kind: "text", value: "x" });
  // @ts-expect-error A reply codec does not take text content.
  await dm.send(replies, { kind: "text", value: "x" });
  // @ts-expect-error A reply codec does not encode text content.
  replies.encode({ kind: "text", value: "x" });
  // @ts-expect-error A delete-message codec does not take text content.
  await group.send(deletions, { kind: "text", value: "x" });
  // @ts-expect-error A delete-message codec does not take text content.
  await dm.send(deletions, { kind: "text", value: "x" });
  // @ts-expect-error A delete-message codec does not encode text content.
  deletions.encode({ kind: "text", value: "x" });

  // @ts-expect-error The send value must be the codec's value type.
  await group.send(pointCodec, { x: "1", y: 2 });
  // @ts-expect-error The send value must be the codec's value type.
  await group.prepareMessage(literalCodec, anyText);
  // @ts-expect-error A codec send needs a value.
  await group.send(pointCodec);
  // @ts-expect-error The reply value must be the codec's value type.
  await message.reply(pointCodec, { x: "1", y: 2 });
  // @ts-expect-error The reply value must be the codec's value type.
  await message.reply(textCodec, 1);
  // @ts-expect-error A wider value must not widen the codec's value type.
  await message.reply(literalCodec, anyText);
}

// verifies: CTYPE-017
// @ts-expect-error A codec's value type does not widen.
export const widened: ContentCodec<string | number> = textCodec;

// verifies: CTYPE-017
export const wrongHook: ContentCodec<Point> = {
  ...pointCodec,
  // @ts-expect-error A fallback hook takes the codec's value type.
  fallback: (text: string) => text,
};
