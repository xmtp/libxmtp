import assert from "node:assert/strict";
import { test } from "node:test";

import {
  DISAPPEARING_ERROR,
  DISAPPEARING_PRESETS,
  DisappearingDurationError,
  MAX_DISAPPEARING_SECONDS,
  disappearingNanoseconds,
  isPresetDuration,
  secondsText,
} from "./disappearing.ts";

test("0 turns it off and seconds become nanoseconds (disappearingRejectsOverflow)", () => {
  assert.equal(disappearingNanoseconds("0"), undefined);
  assert.equal(disappearingNanoseconds("3600"), 3_600_000_000_000n);
  assert.equal(
    disappearingNanoseconds("9223372036"),
    9_223_372_036_000_000_000n,
  );
  assert.equal(MAX_DISAPPEARING_SECONDS, 9_223_372_036n);
});

test("rejects overflow, negatives and non-integers before any write", () => {
  for (const text of [
    "9223372037",
    "-1",
    "",
    " 1",
    "1.5",
    "1e3",
    "abc",
    "99999999999999999999",
  ]) {
    assert.throws(
      () => disappearingNanoseconds(text),
      (error: unknown) =>
        error instanceof DisappearingDurationError &&
        error.message === DISAPPEARING_ERROR,
      text,
    );
  }
  assert.equal(
    DISAPPEARING_ERROR,
    "Enter a whole number of seconds from 0 to 9223372036.",
  );
});

test("presets and the loaded retention", () => {
  assert.deepEqual(
    DISAPPEARING_PRESETS.map((preset) => preset.seconds),
    ["0", "3600", "28800", "86400", "604800"],
  );
  assert.deepEqual(
    DISAPPEARING_PRESETS.map((preset) => preset.label),
    ["Off", "1 hour", "8 hours", "1 day", "1 week"],
  );
  assert.equal(secondsText(28_800_000_000_000n), "28800");
  assert.ok(isPresetDuration("28800"));
  assert.ok(!isPresetDuration("7"));
});
