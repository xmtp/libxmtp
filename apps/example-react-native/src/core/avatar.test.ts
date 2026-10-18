import assert from "node:assert/strict";
import { test } from "node:test";

import { avatarPaletteIndex, groupInitials, personInitials } from "./avatar.ts";

test("palette index is the first SHA-256 byte mod 6 (iOS vectors)", () => {
  // seed → first digest byte → index, from ios-group-screens-design-system.md §2.2.
  const vectors: readonly (readonly [string, number])[] = [
    ["protocol", 4],
    ["maya", 1],
    ["release", 2],
    ["9c", 3],
    ["own", 1],
    ["preview-group", 1],
    ["you", 1],
    ["priya", 2],
    ["marco", 4],
    ["Protocol design", 5],
    ["", 5],
    ["Group", 4],
    ["marco-inbox", 3],
    ["priya-inbox", 5],
    ["lena", 0],
    ["hike", 0],
  ];
  for (const [seed, index] of vectors) {
    assert.equal(
      avatarPaletteIndex(seed),
      index,
      `seed ${JSON.stringify(seed)}`,
    );
  }
});

test("group initials take the first letters of two words after leading @", () => {
  assert.equal(groupInitials("Protocol design"), "PD");
  assert.equal(groupInitials("  hike  club  extra "), "HC");
  assert.equal(groupInitials("@@release"), "R");
  assert.equal(groupInitials(""), "");
  assert.equal(groupInitials("@ marco polo"), "MP");
});

test("person initials: first letter after @, or two inbox ID characters", () => {
  assert.equal(personInitials("@marco#0412"), "M");
  assert.equal(personInitials("nick"), "N");
  assert.equal(personInitials("You"), "Y");
  assert.equal(personInitials("9C0DE1F2", true), "9c");
  assert.equal(personInitials(" @@a "), "A");
  assert.equal(personInitials(""), "");
});
