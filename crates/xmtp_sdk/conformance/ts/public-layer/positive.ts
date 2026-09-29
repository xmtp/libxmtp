// Type-checked with tsc by `sdk lint`: public-layer calls that tsx does not
// check. It compiles only; it does not run.
import {
  Client,
  Dm,
  Group,
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
