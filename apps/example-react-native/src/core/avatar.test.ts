import assert from "node:assert/strict";
import { test } from "node:test";

import {
  AVATAR_PALETTE_SIZE,
  avatarPaletteIndex,
  groupInitials,
  personInitials,
} from "./avatar.ts";

test("each seed gets one palette color, and seeds reach every color", () => {
  const used = new Set<number>();
  for (const seed of [
    "lena",
    "maya",
    "release",
    "9c",
    "protocol",
    "Protocol design",
  ]) {
    const index = avatarPaletteIndex(seed);
    assert.ok(
      Number.isInteger(index) && index >= 0 && index < AVATAR_PALETTE_SIZE,
      seed,
    );
    assert.equal(avatarPaletteIndex(seed), index, `${seed} again`);
    used.add(index);
  }
  assert.equal(used.size, AVATAR_PALETTE_SIZE);
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
