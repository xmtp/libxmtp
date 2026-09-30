import {
  Client,
  ReactionV2Codec,
  TextCodec,
  type ClientOptions,
  type ContentCodec,
  type ContentTypeId,
  type Dm,
  type EncodedContent,
  type Group,
  type Message,
  type MessageId,
  type Signer,
  type StandardContent,
} from "xmtp-sdk";

type Point = { readonly x: number; readonly y: number };

const pointType: ContentTypeId = {
  authorityId: "example.org",
  typeId: "point",
  versionMajor: 1,
  versionMinor: 0,
};

// An app codec with a typed value and both send hooks.
const pointCodec: ContentCodec<Point> = {
  type: pointType,
  encode: (value) => ({
    type: pointType,
    parameters: new Map(),
    content: new TextEncoder().encode(`${value.x},${value.y}`),
  }),
  decode: (encoded) => {
    const [x, y] = new TextDecoder().decode(encoded.content).split(",");
    return { x: Number(x), y: Number(y) };
  },
  fallback: (value) => `point ${value.x},${value.y}`,
  shouldPush: (value) => value.x !== 0,
};

// verifies: CTYPE-017
// Typed codec sends, replies, and mixed registration on the installed package.
export async function consumeTypedCodecs(
  signer: Signer,
  options: ClientOptions,
  group: Group,
  dm: Dm,
  message: Message,
  reaction: Extract<StandardContent, { kind: "reaction" }>,
): Promise<void> {
  const point: Point = { x: 1, y: 2 };
  const sent: MessageId = await group.send(pointCodec, point);
  const text: MessageId = await dm.send(new TextCodec(), "text", {
    shouldPush: false,
  });
  const prepared: MessageId = await group.prepareMessage(pointCodec, point);
  const dmPrepared: MessageId = await dm.prepareMessage(pointCodec, point);
  const reacted: MessageId = await group.send(new ReactionV2Codec(), reaction);
  const reply: MessageId = await message.reply(pointCodec, point);
  const encoded: EncodedContent = pointCodec.encode(point);
  // Codecs of different value types register together.
  const client = await Client.create(signer, {
    ...options,
    codecs: [pointCodec, new TextCodec()],
  });
  await client.end();
  void [sent, text, prepared, dmPrepared, reacted, reply, encoded];
}
