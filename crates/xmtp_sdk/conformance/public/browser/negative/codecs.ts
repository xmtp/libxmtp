import { ReactionV2Codec, TextCodec } from "xmtp-sdk-browser/pure";
import type { Dm, Group, Message } from "xmtp-sdk-browser";

// A typed codec takes only its own value type. Each call must fail to compile.
export async function consumeWrongCodecValues(
  group: Group,
  dm: Dm,
  message: Message,
): Promise<void> {
  new TextCodec().encode(1);
  await group.send(new TextCodec(), 1);
  await dm.send(new TextCodec(), 1);
  await group.prepareMessage(new TextCodec(), 1);
  await dm.prepareMessage(new TextCodec(), 1);
  await message.reply(new TextCodec(), 1);
  await group.send(new ReactionV2Codec(), { kind: "text", value: "x" });
}
