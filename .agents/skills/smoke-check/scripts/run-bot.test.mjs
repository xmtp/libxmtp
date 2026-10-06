import assert from "node:assert/strict";
import { test } from "node:test";

import { replyToTrigger } from "./run-bot.mjs";

for (const [trigger, response] of [
  ["ping", "pong"],
  ["bleep", "bloop"],
]) {
  test(`${trigger} replies to each exact trigger`, async () => {
    const calls = [];
    for (let index = 0; index < 3; index++) {
      const replyId = await replyToTrigger(
        {
          senderInboxId: "peer",
          content: { kind: "text", value: trigger },
          reply: (text) => {
            calls.push(text);
            return Promise.resolve(`reply-${index}`);
          },
        },
        "bot",
        trigger,
        response,
      );
      assert.equal(replyId, `reply-${index}`);
    }
    assert.deepEqual(calls, [response, response, response]);
  });

  test(`${trigger} ignores other text, content kinds, and its own messages`, async () => {
    const ignored = [
      { kind: "text", value: "hello" },
      { kind: "text", value: trigger.toUpperCase() },
      { kind: "text", value: `${trigger}?` },
      { kind: "text", value: `${trigger} ` },
      { kind: "text", value: trigger === "ping" ? "bleep" : "ping" },
      { kind: "reply", body: { kind: "text", value: trigger } },
      { kind: "reply", body: { kind: "text", value: response } },
      { kind: "reaction", content: trigger },
      { kind: "groupUpdated" },
    ];
    const unexpectedReply = () => {
      assert.fail("An ignored message caused a reply");
    };
    for (const content of ignored) {
      assert.equal(
        await replyToTrigger(
          { senderInboxId: "peer", content, reply: unexpectedReply },
          "bot",
          trigger,
          response,
        ),
        undefined,
      );
    }
    assert.equal(
      await replyToTrigger(
        {
          senderInboxId: "bot",
          content: { kind: "text", value: trigger },
          reply: unexpectedReply,
        },
        "bot",
        trigger,
        response,
      ),
      undefined,
    );
  });
}

test("reply failure reaches the stream consumer", async () => {
  const failure = new Error("send failed");
  await assert.rejects(
    replyToTrigger(
      {
        senderInboxId: "peer",
        content: { kind: "text", value: "ping" },
        reply: () => Promise.reject(failure),
      },
      "bot",
      "ping",
      "pong",
    ),
    (error) => error === failure,
  );
});
