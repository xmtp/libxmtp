// Type-checked with tsc by `sdk lint`: public-layer calls that tsx does not
// check. It compiles only; it does not run.
import {
  AttachmentCodec,
  Client,
  DeleteMessageCodec,
  Dm,
  Group,
  MessageStream,
  ReactionV2Codec,
  ReplyCodec,
  TextCodec,
  XmtpError,
  type Attachment,
  type ConnectionState,
  type ErrorCategory,
  type EventStream,
  type MessageContent,
  type ContentCodec,
  type Conversation,
  type EncodedContent,
  type Message,
  type Signer,
  type StandardContent,
} from "../../../../../target/sdk-generated/typescript-napi/index.ts";

type Point = { readonly x: number; readonly y: number };

declare const pointCodec: ContentCodec<Point>;
declare const textCodec: ContentCodec<string>;
declare const literalCodec: ContentCodec<"a" | "b">;
declare const anyText: string;
declare const signer: Signer;

export async function registerTypedCodecs(): Promise<Client> {
  // Typed codecs of different value types register in one list.
  return Client.create(signer, {
    backend: { url: "http://localhost:5050" },
    storage: { location: "inMemory" },
    codecs: [pointCodec, textCodec],
  });
}

export async function replyWithCodec(message: Message): Promise<string> {
  return message.reply(pointCodec, { x: 1, y: 2 });
}

// A standard codec for one StandardContent variant takes only that variant.
export async function standardVariantCodecs(
  group: Group,
  dm: Dm,
  reaction: Extract<StandardContent, { kind: "reaction" }>,
) {
  const reactions = new ReactionV2Codec();
  await group.send(reactions, reaction);
  // @ts-expect-error A reaction codec does not take text content.
  await group.send(reactions, { kind: "text", value: "x" });
  // @ts-expect-error A reaction codec does not take text content.
  await dm.send(reactions, { kind: "text", value: "x" });
  // @ts-expect-error A reaction codec does not encode text content.
  reactions.encode({ kind: "text", value: "x" });
  const replies = new ReplyCodec();
  // @ts-expect-error A reply codec does not take text content.
  await group.send(replies, { kind: "text", value: "x" });
  // @ts-expect-error A reply codec does not take text content.
  await dm.send(replies, { kind: "text", value: "x" });
  // @ts-expect-error A reply codec does not encode text content.
  replies.encode({ kind: "text", value: "x" });
  const deletions = new DeleteMessageCodec();
  // @ts-expect-error A delete-message codec does not take text content.
  await group.send(deletions, { kind: "text", value: "x" });
  // @ts-expect-error A delete-message codec does not take text content.
  await dm.send(deletions, { kind: "text", value: "x" });
  // @ts-expect-error A delete-message codec does not encode text content.
  deletions.encode({ kind: "text", value: "x" });
}

// verifies: CTYPE-017
export async function typedCodecSends(group: Group, encoded: EncodedContent) {
  await group.send(pointCodec, { x: 1, y: 2 });
  await group.send(pointCodec, { x: 1, y: 2 }, { shouldPush: false });
  await group.prepareMessage(textCodec, "prepared");
  // The envelope form keeps working next to the codec form.
  await group.send(encoded);
  await group.send(encoded, { shouldPush: true });
  // @ts-expect-error The send value must be the codec's value type.
  await group.send(pointCodec, { x: "1", y: 2 });
  // @ts-expect-error The send value must be the codec's value type.
  await group.prepareMessage(literalCodec, anyText);
  // @ts-expect-error A codec send needs a value.
  await group.send(pointCodec);
}

// verifies: CTYPE-017
export async function typedCodecHooks(message: Message): Promise<Client> {
  // Optional send hooks keep the codec's value type.
  const noted: ContentCodec<Point> = {
    ...pointCodec,
    fallback: (point) => `point ${point.x},${point.y}`,
    shouldPush: (point) => point.x !== 0,
  };
  await message.reply(noted, { x: 1, y: 2 });
  // @ts-expect-error The reply value must be the codec's value type.
  await message.reply(pointCodec, { x: "1", y: 2 });
  // @ts-expect-error The reply value must be the codec's value type.
  await message.reply(textCodec, 1);
  // @ts-expect-error A wider value must not widen the codec's value type.
  await message.reply(literalCodec, anyText);
  // @ts-expect-error A codec's value type does not widen.
  const widened: ContentCodec<string | number> = textCodec;
  void widened;
  // @ts-expect-error A fallback hook takes the codec's value type.
  const wrongHook: ContentCodec<Point> = {
    ...pointCodec,
    fallback: (text: string) => text,
  };
  void wrongHook;
  // A codec with hooks still registers with others of any value type.
  return Client.create(signer, {
    backend: { url: "http://localhost:5050" },
    storage: { location: "inMemory" },
    codecs: [noted, textCodec],
  });
}

export function readGetters(client: Client, conversation: Conversation) {
  const inboxId: string = client.inboxId;
  const kind: "group" | "dm" = conversation.kind;
  const creator: string | null =
    conversation instanceof Dm ? conversation.creatorInboxId : null;
  const group: Group | undefined =
    conversation instanceof Group ? conversation : undefined;
  const encoded: EncodedContent = pointCodec.encode({ x: 0, y: 0 });
  return { inboxId, kind, creator, group, encoded };
}

export async function streamsAndErrors(client: Client, group: Group) {
  const stream = MessageStream.openGroup(client, group, undefined, {
    onConnectionStateChange: (_previous, current: ConnectionState) =>
      void current,
  });
  for await (const message of stream) {
    const content: MessageContent = message.content;
    void content;
    break;
  }
  const events: EventStream = await client.events({
    kinds: ["conversation.joined"],
    referencesOwnMessages: false,
  });
  void events;
  try {
    await group.sync();
  } catch (error) {
    if (error instanceof XmtpError.ClientClosed) {
      const category: ErrorCategory = error.details.category;
      return category;
    }
  }
  const attachment: Attachment = new AttachmentCodec().decode(
    new TextCodec().encode("text"),
  );
  return attachment.content;
}

// One code narrows to its subclass and keeps the other codes in the else
// branch; its `details.code` is the code literal.
export function errorBranches(error: XmtpError | TypeError): string {
  if (error instanceof XmtpError.ClientClosed) {
    const code: "ClientClosed" = error.details.code;
    return code;
  }
  if (error instanceof XmtpError) return error.details.message;
  return error.message;
}
