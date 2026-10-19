import assert from "node:assert/strict";
import { test } from "node:test";

import { OWN, reactionRecord } from "./chatFakes.ts";
import {
  ChatContentCopy,
  contentText,
  reactionBadges,
} from "./chatMessages.ts";

const removal = { added: false };

test("the latest reaction per sender, emoji and schema wins", () => {
  const badges = reactionBadges(
    [
      reactionRecord("bob", "👍", 3, removal),
      reactionRecord("bob", "👍", 1),
      reactionRecord("ann", "👍", 2),
      reactionRecord("ann", "❤️", 5),
      reactionRecord("ann", "❤️", 6, removal),
      reactionRecord(OWN, "😂", 4),
    ],
    OWN,
  );
  assert.deepEqual(badges, [
    { emoji: "👍", senders: ["ann"], mine: false, count: 1 },
    { emoji: "😂", senders: [OWN], mine: true, count: 1 },
  ]);
});

test("badges group senders sorted and sort by emoji scalar value", () => {
  const badges = reactionBadges(
    [
      reactionRecord("zed", "👍", 1),
      reactionRecord("amy", "👍", 2),
      reactionRecord(OWN, "❤️", 3),
      reactionRecord("bob", "☝🏽", 4),
      reactionRecord("amy", "☝️", 5),
    ],
    OWN,
  );
  // By scalar value U+FE0F (in ☝️) sorts before U+1F3FD (in ☝🏽); by UTF-16
  // code units it sorts after.
  assert.deepEqual(
    badges.map((badge) => [badge.emoji, badge.senders, badge.mine]),
    [
      ["☝️", ["amy"], false],
      ["☝🏽", ["bob"], false],
      ["❤️", [OWN], true],
      ["👍", ["amy", "zed"], false],
    ],
  );
});

test("content text for copy and quotes", () => {
  const file = {
    url: "u",
    contentDigest: "d",
    secret: new Uint8Array(),
    salt: new Uint8Array(),
    nonce: new Uint8Array(),
    scheme: "https",
  };
  assert.equal(contentText({ kind: "text", text: "hi" }), "hi");
  assert.equal(
    contentText({ kind: "reply", quote: "q", body: "body" }),
    "body",
  );
  assert.equal(contentText({ kind: "deleted" }), ChatContentCopy.deleted);
  assert.equal(
    contentText({ kind: "attachment", file: { ...file, filename: "a.pdf" } }),
    "a.pdf",
  );
  assert.equal(contentText({ kind: "attachment", file }), ChatContentCopy.file);
  assert.equal(contentText({ kind: "unsupported", text: "x" }), "x");
  assert.equal(contentText({ kind: "reaction" }), "");
});
