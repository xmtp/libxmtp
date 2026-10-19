import assert from "node:assert/strict";
import { test } from "node:test";

import {
  EmojiCopy,
  RECENT_LIMIT,
  addRecent,
  emojiSections,
  parseEmojiCatalog,
  searchEmoji,
} from "./emojiCatalog.ts";
import { emojiSearchTsv } from "./emojiData.ts";

const catalog = parseEmojiCatalog(emojiSearchTsv);

test("parsing splits on the first tab only and skips lines without one", () => {
  assert.deepEqual(
    parseEmojiCatalog("# header\n#️⃣\tkeycap: #\n😀\tgrinning\tface\n\n🙂\n"),
    [
      { emoji: "#️⃣", name: "keycap: #" },
      { emoji: "😀", name: "grinning\tface" },
    ],
  );
});

test("sections start at the boundary emojis", () => {
  const sections = emojiSections(catalog);
  assert.deepEqual(
    sections.map((section) => [section.title, section.values[0]?.emoji]),
    [
      [EmojiCopy.smileysAndPeople, "😀"],
      [EmojiCopy.animalsAndNature, "🐵"],
      [EmojiCopy.foodAndDrink, "🍇"],
      [EmojiCopy.travelAndPlaces, "🌍"],
      [EmojiCopy.activities, "🎃"],
      [EmojiCopy.objects, "👓"],
      [EmojiCopy.symbols, "🏧"],
      [EmojiCopy.flags, "🏁"],
    ],
  );
  assert.equal(
    sections.reduce((sum, section) => sum + section.values.length, 0),
    catalog.length,
  );
});

test("search matches names ignoring case, or the emoji itself", () => {
  const thumbs = searchEmoji(catalog, "Thumbs Up").map((entry) => entry.emoji);
  assert.ok(thumbs.includes("👍"));
  assert.ok(thumbs.every((emoji) => emoji.startsWith("👍")));
  const dog = searchEmoji(catalog, "dog").map((entry) => entry.name);
  assert.ok(dog.includes("dog face"));
  assert.ok(dog.includes("hot dog"));
  assert.deepEqual(
    searchEmoji(catalog, "🦊").map((entry) => entry.name),
    ["fox"],
  );
  assert.equal(searchEmoji(catalog, "").length, catalog.length);
});

test("recents put the newest first, drop duplicates and keep 30", () => {
  let recent: readonly string[] = [];
  recent = addRecent(recent, "👍");
  recent = addRecent(recent, "❤️");
  recent = addRecent(recent, "👍");
  assert.deepEqual(recent, ["👍", "❤️"]);
  for (let index = 0; index < 40; index += 1) {
    recent = addRecent(recent, String(index));
  }
  assert.equal(recent.length, RECENT_LIMIT);
  assert.equal(recent[0], "39");
});
