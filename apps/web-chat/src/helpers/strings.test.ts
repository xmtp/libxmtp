import { describe, expect, it } from "vitest";

import { isValidHandle } from "./handles";
import { asciiLowercase, normalizeMemberInput } from "./strings";

// U+212A KELVIN SIGN and U+0130 LATIN CAPITAL LETTER I WITH DOT ABOVE:
// `toLowerCase` folds the first to ASCII "k"
const KELVIN = "K";
const DOTTED_I = "İ";

describe("asciiLowercase", () => {
  it("lowercases ASCII capitals", () => {
    expect(asciiLowercase("Nick#2341")).toBe("nick#2341");
    expect(asciiLowercase(`0x${"AB".repeat(20)}`)).toBe(`0x${"ab".repeat(20)}`);
  });

  it.each([`${KELVIN}ate`, `${DOTTED_I}nci`])(
    "keeps the non-ASCII letter in %s, so the grammar refuses it",
    (input) => {
      expect(asciiLowercase(input)).toBe(input);
      expect(isValidHandle(asciiLowercase(input))).toBe(false);
    },
  );
});

describe("normalizeMemberInput", () => {
  it("lowercases ASCII capitals and removes each space", () => {
    expect(normalizeMemberInput(" Ni ck #2341 ")).toBe("nick#2341");
    expect(normalizeMemberInput(`${KELVIN}ate`)).toBe(`${KELVIN}ate`);
  });
});
