import assert from "node:assert/strict";
import { test } from "node:test";

import { abbreviatedDate, messageDateTime } from "./dates.ts";

/** Apple's formatters put a narrow no-break space before AM and PM. */
const nbsp = "\u202f";

// Local-time constructors keep these independent of the machine time zone.

test("message date and time: Oct 14 at 4:03 AM", () => {
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
});

test("abbreviated dates: Oct 18, 2026", () => {
  assert.equal(abbreviatedDate(new Date(2026, 9, 18, 15, 0)), "Oct 18, 2026");
  assert.equal(abbreviatedDate(new Date(2027, 1, 3)), "Feb 3, 2027");
});
