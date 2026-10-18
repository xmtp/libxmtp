import assert from "node:assert/strict";
import { test } from "node:test";

import {
  NARROW_NO_BREAK_SPACE as nbsp,
  abbreviatedDate,
  dateFromNanoseconds,
  messageDateTime,
  sameLocalDay,
  timeOfDay,
} from "./dates.ts";

// Local-time constructors keep these independent of the machine time zone.

test("message date and time reads like SwiftUI: Oct 14 at 4:03 AM", () => {
  assert.equal(
    messageDateTime(new Date(2026, 9, 14, 4, 3)),
    `Oct 14 at 4:03${nbsp}AM`,
  );
  assert.equal(
    messageDateTime(new Date(2026, 0, 1, 0, 0)),
    `Jan 1 at 12:00${nbsp}AM`,
  );
  assert.equal(
    messageDateTime(new Date(2026, 11, 31, 12, 30)),
    `Dec 31 at 12:30${nbsp}PM`,
  );
  assert.equal(
    messageDateTime(new Date(2026, 4, 9, 23, 59)),
    `May 9 at 11:59${nbsp}PM`,
  );
  assert.equal(nbsp, "\u202f");
});

test("row time is the 12-hour time only", () => {
  assert.equal(timeOfDay(new Date(2026, 9, 18, 16, 3)), `4:03${nbsp}PM`);
  assert.equal(timeOfDay(new Date(2026, 9, 18, 9, 41)), `9:41${nbsp}AM`);
  assert.equal(timeOfDay(new Date(2026, 9, 18, 12, 0)), `12:00${nbsp}PM`);
});

test("abbreviated dates: Oct 18, 2026", () => {
  assert.equal(abbreviatedDate(new Date(2026, 9, 18, 15, 0)), "Oct 18, 2026");
  assert.equal(abbreviatedDate(new Date(2027, 1, 3)), "Feb 3, 2027");
});

test("same local day and nanosecond conversion", () => {
  assert.ok(
    sameLocalDay(new Date(2026, 9, 18, 0, 0), new Date(2026, 9, 18, 23, 59)),
  );
  assert.ok(
    !sameLocalDay(new Date(2026, 9, 18, 23, 59), new Date(2026, 9, 19, 0, 0)),
  );
  assert.ok(!sameLocalDay(new Date(2025, 9, 18), new Date(2026, 9, 18)));
  assert.equal(
    dateFromNanoseconds(1_760_000_000_123_456_789n).getTime(),
    1_760_000_000_123,
  );
});
