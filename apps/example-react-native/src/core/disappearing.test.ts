import assert from "node:assert/strict";
import { test } from "node:test";

import {
  DISAPPEARING_PRESETS,
  DisappearingDurationError,
  disappearingNanoseconds,
  isPresetDuration,
  secondsText,
} from "./disappearing.ts";

test("typed seconds from 0 (off) to 9223372036 become nanoseconds; anything else is rejected", () => {
  assert.equal(disappearingNanoseconds("0"), undefined);
  assert.equal(disappearingNanoseconds("3600"), 3_600_000_000_000n);
  assert.equal(
    disappearingNanoseconds("9223372036"),
    9_223_372_036_000_000_000n,
  );
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
      DisappearingDurationError,
      text,
    );
  }
});

test("a loaded retention selects its preset, else Custom", () => {
  assert.deepEqual(
    DISAPPEARING_PRESETS.map((preset) => preset.seconds),
    ["0", "3600", "28800", "86400", "604800"],
  );
  for (const preset of DISAPPEARING_PRESETS) {
    const retention = disappearingNanoseconds(preset.seconds) ?? 0n;
    assert.ok(isPresetDuration(secondsText(retention)), preset.seconds);
  }
  assert.equal(secondsText(28_800_999_999_999n), "28800");
  assert.ok(!isPresetDuration("7"));
});
