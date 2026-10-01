import { ReadingCodec, type Reading } from "@example/reading-codec";
import type { Dm, Group, Message } from "xmtp-sdk";

export async function typedCalls(
  group: Group,
  dm: Dm,
  message: Message,
): Promise<void> {
  const codec = new ReadingCodec();
  const value: Reading = { text: "typed value", revision: 3 };
  codec.decode(codec.encode(value));
  await group.send(codec, value);
  await dm.send(codec, value);
  await group.prepareMessage(codec, value);
  await dm.prepareMessage(codec, value);
  await message.reply(codec, value);
}
