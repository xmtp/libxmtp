import { ReadingCodec } from "@example/reading-codec";
import type { Dm, Group, Message } from "xmtp-sdk";

declare const group: Group;
declare const dm: Dm;
declare const message: Message;
const codec = new ReadingCodec();

codec.encode(7);
void group.send(codec, 7);
void dm.send(codec, 7);
void group.prepareMessage(codec, 7);
void dm.prepareMessage(codec, 7);
void message.reply(codec, 7);
