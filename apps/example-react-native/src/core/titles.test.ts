import assert from "node:assert/strict";
import { test } from "node:test";

import {
  conversationInitials,
  conversationTitle,
  detailsAvatarLabel,
  displayName,
  logicalKey,
  memberInitial,
  memberTitle,
  rowMatches,
  shortInboxId,
  splitHandle,
  stripAt,
  TitleCopy,
} from "./titles.ts";

const dm = {
  id: "abc",
  kind: "dm" as const,
  peerInboxId: "123456789012345",
  peerHandle: "nick#1234",
};

test("a conversation's title is its alias, group name, handle or short inbox ID", () => {
  assert.equal(conversationTitle(dm), "@nick#1234");
  assert.equal(conversationTitle(dm, "Local name"), "Local name");
  assert.equal(conversationTitle(dm, "   "), "@nick#1234");
  assert.equal(conversationTitle({ id: "g", kind: "group" }), TitleCopy.group);
  assert.equal(
    conversationTitle({ id: "g", kind: "group", name: "  " }),
    TitleCopy.group,
  );
  assert.equal(
    conversationTitle({ id: "g", kind: "group", name: "Hike" }),
    "Hike",
  );
  assert.equal(
    conversationTitle({ id: "d", kind: "dm" }),
    TitleCopy.directMessage,
  );
  assert.equal(
    conversationTitle({ id: "d", kind: "dm", peerInboxId: "123456789012345" }),
    "123456789012",
  );
  assert.equal(
    conversationTitle({ id: "d", kind: "dm", peerInboxId: "", peerHandle: "" }),
    TitleCopy.directMessage,
  );
});

test("row and Details initials use the inbox ID rule only without an alias or handle", () => {
  const noHandle = {
    id: "d",
    kind: "dm" as const,
    peerInboxId: "9c0de1ab34cd56ef",
  };
  // Conversation rows.
  assert.equal(conversationInitials(dm, conversationTitle(dm)), "N");
  assert.equal(
    conversationInitials(noHandle, conversationTitle(noHandle)),
    "9c",
  );
  assert.equal(conversationInitials(noHandle, "Alias"), "A");
  assert.equal(
    conversationInitials({ id: "g", kind: "group" }, "Protocol design"),
    "PD",
  );
  // The Details header.
  assert.equal(detailsAvatarLabel(noHandle), "9c");
  assert.equal(detailsAvatarLabel(noHandle, "zed"), "Z");
  assert.equal(detailsAvatarLabel(dm), "N");
  assert.equal(
    detailsAvatarLabel({ id: "g", kind: "group", name: "a b c" }),
    "AB",
  );
});

test("short inbox IDs and display names", () => {
  const inbox = "abcdef0123456789abcdef";
  assert.equal(shortInboxId(inbox), "abcdef012345");
  assert.equal(shortInboxId(inbox, true), "abcdef01");
  assert.equal(displayName(inbox, "marco"), "@marco");
  assert.equal(displayName(inbox, undefined), "abcdef012345");
  assert.equal(displayName(inbox, undefined, true), "abcdef01");
  assert.equal(memberTitle(inbox), "abcdef012345");
  assert.equal(memberTitle(inbox, "marco#0412"), "marco#0412");
  assert.equal(memberInitial(inbox, "marco#0412"), "M");
  assert.equal(memberInitial(inbox), "A");
});

test("logical keys sort DM inbox IDs so duplicate DMs share one key", () => {
  assert.equal(
    logicalKey({ id: "group-id", kind: "group" }, "own"),
    "group-id",
  );
  assert.equal(
    logicalKey({ id: "x", kind: "dm", peerInboxId: "zzz" }, "aaa"),
    "dm:aaa:zzz",
  );
  assert.equal(
    logicalKey({ id: "y", kind: "dm", peerInboxId: "aaa" }, "zzz"),
    "dm:aaa:zzz",
  );
  assert.equal(logicalKey({ id: "z", kind: "dm" }, "own"), "dm::own");
});

test("handles split once at the first # after dropping exactly one leading @", () => {
  assert.deepEqual(splitHandle("@marco#0412"), {
    name: "marco",
    code: "#0412",
  });
  assert.deepEqual(splitHandle("lena#2207"), { name: "lena", code: "#2207" });
  assert.deepEqual(splitHandle("nick"), { name: "nick", code: "" });
  assert.deepEqual(splitHandle(""), { name: "", code: "" });
  assert.deepEqual(splitHandle("a#b#c"), { name: "a", code: "#b#c" });
  assert.deepEqual(splitHandle("@@nick"), { name: "@nick", code: "" });
  // The spoken label (`stripAt`) follows the same one-@ rule.
  assert.equal(stripAt("@nick"), "nick");
  assert.equal(stripAt("@@nick"), "@nick");
  assert.equal(stripAt("nick"), "nick");
});

test("search matches title, handle, preview or ID ignoring case", () => {
  // Each field holds a different word, so each search proves one field.
  const row = {
    title: "Book club",
    peerHandle: "nick#1234",
    preview: "hello",
    id: "abc",
  };
  for (const search of ["BOOK", "NICK", "HELLO", "ABC"]) {
    assert.ok(rowMatches(row, search), search);
  }
  assert.ok(!rowMatches(row, "zzz"));
});
