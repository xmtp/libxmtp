// Type-checked with tsc by `sdk lint`: public-layer calls that tsx does not
// check. It compiles only; it does not run.
import {
  AttachmentCodec,
  Client,
  Dm,
  Group,
  MessageStream,
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
} from "../../../../../target/sdk-generated/typescript-napi/index.ts";

type Point = { readonly x: number; readonly y: number };

declare const pointCodec: ContentCodec<Point>;
declare const textCodec: ContentCodec<string>;
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
  // @ts-expect-error A fallback hook takes the codec's value type.
  const wrongHook: ContentCodec<Point> = { ...pointCodec, fallback: (text: string) => text };
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
    kinds: ["conversationJoined"],
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
