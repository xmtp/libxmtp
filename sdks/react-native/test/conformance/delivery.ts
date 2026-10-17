import {
  type Client,
  type MessageStream,
  XmtpError,
} from "@xmtp/react-native-sdk";

import { assert, equal } from "./assertions";
import { within } from "./async";

/** Runs through real public readers and the database's stored acknowledgement. */
export async function deliveryJourney(client: Client): Promise<string[]> {
  const group = await client.conversations.createGroup([]);
  await group.sendText("delivery first");
  await group.sendText("delivery second");
  await group.sendText("delivery third");
  const beginning = await client.conversations.beginningDeliveryCursor();
  const readers: MessageStream[] = [];
  const open = () => {
    const reader = group.streamMessages();
    readers.push(reader);
    return reader;
  };
  const next = async (reader: MessageStream) => {
    const item = await within(reader.next(), "durable message read");
    assert(!item.done, "reader ended before its retained message");
    return item.value;
  };
  try {
    const original = open();
    const first = await next(original);
    const competing = open();
    let ownership: unknown;
    try {
      await within(competing.next(), "second default reader rejection");
    } catch (error) {
      ownership = error;
    }
    assert(
      ownership instanceof XmtpError.ConsumerOwned,
      "a second native default reader must fail with ConsumerOwned",
    );
    await original.return();
    const afterReturn = open();
    equal(
      (await next(afterReturn)).id,
      first.id,
      "return must leave the delivered item unacknowledged",
    );
    const second = await next(afterReturn);
    assert(
      second.id !== first.id,
      "next must advance after acknowledging the prior item",
    );
    await afterReturn.end();
    const afterEnd = open();
    equal(
      (await next(afterEnd)).id,
      second.id,
      "end must retain the latest item while the prior next acknowledgement persists",
    );
    await afterEnd.end();

    const failed = open();
    const failure = new Error("app processing failed");
    let seen = 0;
    let rejected: unknown;
    try {
      await within(
        failed.onValue(async (message) => {
          seen++;
          equal(
            message.id,
            second.id,
            "failed callback must receive unacknowledged item",
          );
          await client.conversations.getMessageById(message.id);
          throw failure;
        }),
        "reentrant rejected message callback",
      );
    } catch (error) {
      rejected = error;
    }
    equal(rejected, failure, "callback mode must preserve the app's failure");
    equal(seen, 1, "callback failure must not consume the next native item");

    const replay = group.streamMessages({ from: beginning });
    readers.push(replay);
    equal(
      (await next(replay)).id,
      first.id,
      "explicit replay must include the acknowledged first item",
    );
    await replay.end();
    const replacement = open();
    equal(
      (await next(replacement)).id,
      second.id,
      "failed callback and independent replay must not change default progress",
    );
    await replacement.end();

    // Start then immediately end the real asynchronous opener. This covers the
    // ordinary cancel race; native delayed-open tests retain deterministic late-open proof.
    const cancelled = open();
    const pending = cancelled.next();
    await cancelled.end();
    await within(pending, "cancelled native read settles");
    const afterCancel = open();
    equal(
      (await next(afterCancel)).id,
      second.id,
      "cancelled opening must release ownership without acknowledging an item",
    );
    return [
      "native durable next/return/end acknowledgement, default ownership, callback failure, cancellation and independent replay",
    ];
  } finally {
    await Promise.all(readers.map((reader) => reader.end()));
  }
}
