import assert from "node:assert/strict";
import { test } from "node:test";

import { OWN, PEER, textMessage } from "./chatFakes.ts";
import { type ChatMessage, type ReactionBadgeModel } from "./chatMessages.ts";
import {
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

test("the menu shows Copy always, Reply and React when allowed and not deleted", () => {
  assert.deepEqual(menuItems(peerRow, OWN, true), ["copy", "reply", "react"]);
  assert.deepEqual(menuItems(peerRow, OWN, false), ["copy"]);
  assert.deepEqual(menuItems(deletedPeer, OWN, true), ["copy"]);
});

test("own published rows offer Delete; own unpublished rows Retry and Edit", () => {
  assert.deepEqual(menuItems(ownRow, OWN, true), [
    "copy",
    "reply",
    "react",
    "delete",
  ]);
  assert.deepEqual(menuItems(ownQueued, OWN, true), [
    "copy",
    "reply",
    "react",
    "retry",
    "edit",
  ]);
  assert.deepEqual(menuItems(ownFailed, OWN, true), [
    "copy",
    "reply",
    "react",
    "retry",
    "edit",
  ]);
  // A request (consent not allowed) offers Copy only, even on own rows.
  assert.deepEqual(menuItems(ownRow, OWN, false), ["copy"]);
  assert.ok(!menuItems(peerRow, OWN, true).includes("delete"));
});

test("VoiceOver actions: Reply, React, Copy, then Delete on own published rows", () => {
  assert.deepEqual(accessibilityItems(ownRow, OWN, true), [
    "reply",
    "react",
    "copy",
    "delete",
  ]);
  assert.deepEqual(accessibilityItems(ownQueued, OWN, true), [
    "reply",
    "react",
    "copy",
  ]);
  assert.deepEqual(accessibilityItems(peerRow, OWN, false), ["copy"]);
});

test("a swipe arms past 48 pt sideways, more horizontal than vertical, either way", () => {
  assert.equal(swipeArmed(48, 0, true), true);
  assert.equal(swipeArmed(-60, 10, true), true);
  assert.equal(swipeArmed(47, 0, true), false);
  assert.equal(swipeArmed(60, 60, true), false);
  assert.equal(swipeArmed(60, -61, true), false);
  assert.equal(swipeArmed(80, 0, false), false);
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
