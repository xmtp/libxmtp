import assert from "node:assert/strict";
import { test } from "node:test";

import { OWN, PEER, textMessage } from "./chatFakes.ts";
import { type ChatMessage, type ReactionBadgeModel } from "./chatMessages.ts";
import {
  type MessageMenuItem,
  SWIPE_REPLY_DISTANCE,
  accessibilityItems,
  canReply,
  flightTarget,
  menuItems,
  reactionRows,
  reactionTotal,
  removableReaction,
  swipeArmed,
} from "./messageInteractions.ts";

const peerRow = textMessage(1);
const ownRow = textMessage(2, { sender: OWN });
const ownQueued: ChatMessage = {
  ...textMessage(3, { sender: OWN }),
  delivery: "queued",
};
const ownFailed: ChatMessage = { ...ownQueued, delivery: "failed" };
const deletedPeer: ChatMessage = {
  ...textMessage(4),
  content: { kind: "deleted" },
};

test("each row offers its context-menu and VoiceOver actions", () => {
  const cases: readonly (readonly [
    string,
    ChatMessage,
    boolean,
    readonly MessageMenuItem[],
    readonly MessageMenuItem[],
  ])[] = [
    [
      "peer",
      peerRow,
      true,
      ["copy", "reply", "react"],
      ["reply", "react", "copy"],
    ],
    ["peer, request", peerRow, false, ["copy"], ["copy"]],
    ["deleted peer", deletedPeer, true, ["copy"], ["copy"]],
    [
      "own published",
      ownRow,
      true,
      ["copy", "reply", "react", "delete"],
      ["reply", "react", "copy", "delete"],
    ],
    [
      "own queued",
      ownQueued,
      true,
      ["copy", "reply", "react", "retry", "edit"],
      ["reply", "react", "copy"],
    ],
    [
      "own failed",
      ownFailed,
      true,
      ["copy", "reply", "react", "retry", "edit"],
      ["reply", "react", "copy"],
    ],
    ["own, request", ownRow, false, ["copy"], ["copy"]],
  ];
  for (const [name, row, allowed, menu, voiceOver] of cases) {
    assert.deepEqual(menuItems(row, OWN, allowed), menu, name);
    assert.deepEqual(accessibilityItems(row, OWN, allowed), voiceOver, name);
  }
});

test("a swipe arms past the reply distance sideways, more horizontal than vertical, either way", () => {
  const d = SWIPE_REPLY_DISTANCE;
  assert.equal(swipeArmed(d, 0, true), true);
  assert.equal(swipeArmed(-(d + 12), 10, true), true);
  assert.equal(swipeArmed(d - 1, 0, true), false);
  assert.equal(swipeArmed(d + 12, d + 12, true), false);
  assert.equal(swipeArmed(d + 12, -(d + 13), true), false);
  assert.equal(swipeArmed(d + 32, 0, false), false);
  assert.equal(canReply(peerRow, true), true);
  assert.equal(canReply(deletedPeer, true), false);
  assert.equal(canReply(peerRow, false), false);
});

test("the flight lands on the newest row when it is own with the submitted text", () => {
  const sent = textMessage(5, { sender: OWN, text: "hello" });
  assert.equal(flightTarget([peerRow, sent], OWN, "hello"), sent);
  assert.equal(flightTarget([peerRow, sent], OWN, "other"), undefined);
  assert.equal(flightTarget([peerRow, sent], OWN, undefined), undefined);
  const fromPeer = textMessage(6, { sender: PEER, text: "hello" });
  assert.equal(flightTarget([sent, fromPeer], OWN, "hello"), undefined);
  assert.equal(flightTarget([], OWN, "hello"), undefined);
});

const badges: ReactionBadgeModel[] = [
  { emoji: "❤️", senders: ["a", OWN], mine: true, count: 2 },
  { emoji: "👍", senders: ["b"], mine: false, count: 1 },
];

test("the Reactions sheet lists every sender under All, else one emoji", () => {
  assert.equal(reactionTotal(badges), 3);
  assert.deepEqual(reactionRows(badges, ""), [
    { sender: "a", emoji: "❤️" },
    { sender: OWN, emoji: "❤️" },
    { sender: "b", emoji: "👍" },
  ]);
  assert.deepEqual(reactionRows(badges, "👍"), [{ sender: "b", emoji: "👍" }]);
});

test("Remove my reaction shows for an own reaction in the filter when allowed", () => {
  assert.equal(removableReaction(badges, "", true), "❤️");
  assert.equal(removableReaction(badges, "❤️", true), "❤️");
  assert.equal(removableReaction(badges, "👍", true), undefined);
  assert.equal(removableReaction(badges, "", false), undefined);
});
