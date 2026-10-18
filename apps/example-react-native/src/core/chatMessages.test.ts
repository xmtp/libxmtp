import assert from "node:assert/strict";
import { test } from "node:test";

import { OWN, textMessage } from "./chatFakes.ts";
import {
  type ReactionRecord,
  compareScalars,
  contentText,
  hasOwnReaction,
  reactionBadges,
} from "./chatMessages.ts";

const record = (
  sender: string,
  emoji: string,
  sentAt: number,
  added = true,
  schema = "unicode",
): ReactionRecord => ({
  sender,
  emoji,
  schema,
  added,
  sentAtNs: BigInt(sentAt),
});

test("the latest reaction per sender, emoji and schema wins", () => {
  const badges = reactionBadges(
    [
      record("bob", "👍", 3, false),
      record("bob", "👍", 1),
      record("ann", "👍", 2),
      record("ann", "❤️", 5),
      record("ann", "❤️", 6, false),
      record(OWN, "😂", 4),
    ],
    OWN,
  );
  assert.deepEqual(badges, [
    { emoji: "👍", senders: ["ann"], mine: false, count: 1 },
    { emoji: "😂", senders: [OWN], mine: true, count: 1 },
  ]);
});

test("equal times keep the input order", () => {
  const badges = reactionBadges(
    [record("bob", "👍", 1, true), record("bob", "👍", 1, false)],
    OWN,
  );
  assert.deepEqual(badges, []);
  const readded = reactionBadges(
    [record("bob", "👍", 1, false), record("bob", "👍", 1, true)],
    OWN,
  );
  assert.equal(readded[0]?.count, 1);
});

test("badges group senders sorted and sort by emoji scalar value", () => {
  const badges = reactionBadges(
    [record("zed", "👍", 1), record("amy", "👍", 2), record(OWN, "❤️", 3)],
    OWN,
  );
  assert.deepEqual(
    badges.map((badge) => [badge.emoji, badge.senders, badge.mine]),
    [
      ["❤️", [OWN], true],
      ["👍", ["amy", "zed"], false],
    ],
  );
});

test("scalar order differs from UTF-16 order for private-use characters", () => {
  // U+E000 sorts after U+1F44D in UTF-16 code units, before it by scalar value.
  assert.ok(compareScalars("\u{E000}", "👍") < 0);
  assert.ok(compareScalars("a", "ab") < 0);
  assert.equal(compareScalars("👍", "👍"), 0);
});

test("own reactions toggle", () => {
  const message = {
    ...textMessage(1),
    reactions: [record(OWN, "👍", 1), record("bob", "❤️", 2)],
  };
  assert.equal(hasOwnReaction(message, "👍", OWN), true);
  assert.equal(hasOwnReaction(message, "❤️", OWN), false);
});

test("content text for copy and quotes", () => {
  assert.equal(contentText({ kind: "text", text: "hi" }), "hi");
  assert.equal(
    contentText({ kind: "reply", quote: "q", body: "body" }),
    "body",
  );
  assert.equal(contentText({ kind: "deleted" }), "Message deleted");
  assert.equal(
    contentText({
      kind: "attachment",
      file: {
        url: "u",
        contentDigest: "d",
        secret: new Uint8Array(),
        salt: new Uint8Array(),
        nonce: new Uint8Array(),
        scheme: "https",
      },
    }),
    "File",
  );
  assert.equal(contentText({ kind: "unsupported", text: "x" }), "x");
  assert.equal(contentText({ kind: "reaction" }), "");
});
