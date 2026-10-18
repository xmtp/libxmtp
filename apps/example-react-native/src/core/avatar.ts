// Avatar color and initials rules from the iOS example
// (DesignSystem/Identity.swift and ExampleCore/Identity/DisplayInitials.swift).

import { sha256, utf8 } from "./sha256.ts";

/** The number of avatar color pairs (`AvatarBackground0`…`AvatarBackground5`). */
export const AVATAR_PALETTE_SIZE = 6;

/** The palette index for `seed`: the first SHA-256 byte of its UTF-8, mod 6. */
export function avatarPaletteIndex(seed: string): number {
  return (sha256(utf8(seed)).at(0) ?? 0) % AVATAR_PALETTE_SIZE;
}

/** Drops every leading `@`. */
function dropLeadingAt(text: string): string {
  let index = 0;
  while (text[index] === "@") index += 1;
  return text.slice(index);
}

/** The first character (code point) of `text`, or an empty string. */
function firstCharacter(text: string): string {
  const first = text.codePointAt(0);
  return first === undefined ? "" : String.fromCodePoint(first);
}

/**
 * Group initials: the first letters of the first two whitespace-separated
 * words, after leading `@`s, uppercased. `"Protocol design"` → `"PD"`.
 */
export function groupInitials(title: string): string {
  return dropLeadingAt(title.trim())
    .split(/\s+/u)
    .filter((word) => word.length > 0)
    .slice(0, 2)
    .map(firstCharacter)
    .join("")
    .toUpperCase();
}

/**
 * Person initials: for an inbox ID, its first two characters lowercased;
 * otherwise the first character after leading `@`s, uppercased.
 */
export function personInitials(label: string, isInboxId = false): string {
  const value = label.trim();
  if (isInboxId) return Array.from(value).slice(0, 2).join("").toLowerCase();
  return firstCharacter(dropLeadingAt(value)).toUpperCase();
}
