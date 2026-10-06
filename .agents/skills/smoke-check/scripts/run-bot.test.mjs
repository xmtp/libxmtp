import assert from "node:assert/strict";
import { test } from "node:test";

import { receivedEvent, replyToTrigger } from "./run-bot.mjs";

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

test("group rename logs preserve the old and new names and message IDs", () => {
  const record = JSON.parse(
    JSON.stringify(
      receivedEvent({
        id: "rename-message",
        conversationId: "smoke-group",
        senderInboxId: "owner",
        sentAt: { ns: 123n },
        content: {
          kind: "groupUpdated",
          value: {
            initiatedByInboxId: "owner",
            addedInboxes: [],
            removedInboxes: [],
            leftInboxes: [],
            metadataFieldChanges: [
              {
                fieldName: "group_name",
                oldValue: "Local smoke test",
                newValue: "Renamed smoke test",
              },
            ],
            addedAdminInboxes: [],
            removedAdminInboxes: [],
            addedSuperAdminInboxes: [],
            removedSuperAdminInboxes: [],
          },
        },
      }),
    ),
  );
  assert.equal(record.event, "received");
  assert.equal(record.id, "rename-message");
  assert.equal(record.conversationId, "smoke-group");
  assert.equal(record.senderInboxId, "owner");
  assert.equal(record.contentKind, "groupUpdated");
  assert.deepEqual(record.groupUpdated?.metadataFieldChanges, [
    {
      fieldName: "group_name",
      oldValue: "Local smoke test",
      newValue: "Renamed smoke test",
    },
  ]);
  assert.equal(record.groupUpdated?.initiatedByInboxId, "owner");
});
