// The emoji picker's catalog, sections, search and recents.

/** One catalog entry: the emoji and its Unicode name. */
export interface EmojiEntry {
  readonly emoji: string;
  readonly name: string;
}

export interface EmojiSection {
  readonly title: string;
  readonly values: readonly EmojiEntry[];
}

export const EmojiCopy = {
  title: "Choose a reaction",
  close: "Close",
  search: "Search emoji",
  recentlyUsed: "Recently used",
  smileysAndPeople: "Smileys and people",
  animalsAndNature: "Animals and nature",
  foodAndDrink: "Food and drink",
  travelAndPlaces: "Travel and places",
  activities: "Activities",
  objects: "Objects",
  symbols: "Symbols",
  flags: "Flags",
} as const;

/** The first section's title; the boundaries below start the others. */
const FIRST_SECTION = EmojiCopy.smileysAndPeople;

/** A new section starts at each of these emojis (`sections`). */
const SECTION_STARTS: ReadonlyMap<string, string> = new Map([
  ["🐵", EmojiCopy.animalsAndNature],
  ["🍇", EmojiCopy.foodAndDrink],
  ["🌍", EmojiCopy.travelAndPlaces],
  ["🎃", EmojiCopy.activities],
  ["👓", EmojiCopy.objects],
  ["🏧", EmojiCopy.symbols],
  ["🏁", EmojiCopy.flags],
]);

/** "Recently used" keeps this many emojis, newest first. */
export const RECENT_LIMIT = 30;

/**
 * Parses `emoji<TAB>name` lines, split on the first tab. Lines without a tab
 * (the "#" header comments) are skipped; `#️⃣	keycap: #` is an entry.
 */
export function parseEmojiCatalog(text: string): EmojiEntry[] {
  const entries: EmojiEntry[] = [];
  for (const line of text.split("\n")) {
    const tab = line.indexOf("\t");
    if (tab <= 0 || tab === line.length - 1) continue;
    entries.push({ emoji: line.slice(0, tab), name: line.slice(tab + 1) });
  }
  return entries;
}

/** The catalog in sections: "Smileys and people", then one per boundary emoji. */
export function emojiSections(catalog: readonly EmojiEntry[]): EmojiSection[] {
  const sections: { title: string; values: EmojiEntry[] }[] = [
    { title: FIRST_SECTION, values: [] },
  ];
  for (const entry of catalog) {
    const title = SECTION_STARTS.get(entry.emoji);
    if (title !== undefined) sections.push({ title, values: [] });
    sections[sections.length - 1]?.values.push(entry);
  }
  return sections;
}

/**
 * Search: the name contains the query, ignoring case, or the emoji contains
 * it. An empty query matches everything.
 */
export function searchEmoji(
  catalog: readonly EmojiEntry[],
  query: string,
): readonly EmojiEntry[] {
  if (query === "") return catalog;
  const needle = query.toLocaleLowerCase();
  return catalog.filter(
    (entry) =>
      entry.name.toLocaleLowerCase().includes(needle) ||
      entry.emoji.includes(query),
  );
}

/** Puts `emoji` first, without duplicates, keeping at most `RECENT_LIMIT`. */
export function addRecent(
  recent: readonly string[],
  emoji: string,
): readonly string[] {
  return [emoji, ...recent.filter((value) => value !== emoji)].slice(
    0,
    RECENT_LIMIT,
  );
}
