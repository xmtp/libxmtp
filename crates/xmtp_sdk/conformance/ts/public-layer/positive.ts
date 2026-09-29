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
} from "../../../../../target/sdk-generated/typescript-napi/public-api.gen.ts";

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
