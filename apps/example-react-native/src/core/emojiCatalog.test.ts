import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { test } from "node:test";

import {
  RECENT_LIMIT,
  addRecent,
  emojiSections,
  parseEmojiCatalog,
  searchEmoji,
} from "./emojiCatalog.ts";
import { emojiSearchTsv } from "./emojiData.ts";

const catalog = parseEmojiCatalog(emojiSearchTsv);

test("the catalog keeps every entry and skips the header comments", () => {
  assert.equal(catalog.length, 3781);
  assert.deepEqual(catalog[0], { emoji: "😀", name: "grinning face" });
  assert.ok(catalog.some((entry) => entry.name === "keycap: #"));
  assert.deepEqual(catalog.at(-1), { emoji: "🏴󠁧󠁢󠁷󠁬󠁳󠁿", name: "flag: Wales" });
  assert.ok(catalog.every((entry) => !entry.emoji.startsWith("# ")));
});

test("parsing splits on the first tab only and skips lines without one", () => {
  assert.deepEqual(parseEmojiCatalog("# header\n😀\tgrinning\tface\n\n🙂\n"), [
    { emoji: "😀", name: "grinning\tface" },
  ]);
});

test("the data module matches the iOS example's emoji-search.tsv", () => {
  const source = new URL(
    "../../../example-ios/ExampleApp/Features/Chat/emoji-search.tsv",
    import.meta.url,
  );
  // A moved or renamed iOS file must fail here, not turn the guard off.
  assert.ok(existsSync(source), `missing ${source.pathname}`);
  assert.equal(emojiSearchTsv, readFileSync(source, "utf8"));
});

test("sections start at the boundary emojis", () => {
  const sections = emojiSections(catalog);
  assert.deepEqual(
    sections.map((section) => [section.title, section.values[0]?.emoji]),
    [
      ["Smileys and people", "😀"],
      ["Animals and nature", "🐵"],
      ["Food and drink", "🍇"],
      ["Travel and places", "🌍"],
      ["Activities", "🎃"],
      ["Objects", "👓"],
      ["Symbols", "🏧"],
      ["Flags", "🏁"],
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
