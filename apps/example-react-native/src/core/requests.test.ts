import assert from "node:assert/strict";
import { test } from "node:test";

import {
  REQUEST_COUNT_LIMIT,
  detailWindow,
  requestBadgeCount,
  shouldLoadNextPage,
} from "./requests.ts";

const ids = (count: number): string[] =>
  Array.from({ length: count }, (_, index) => `request-${String(index)}`);

test("the badge counts up to 500 requests and hides at the 501 cap (conversationListRules)", () => {
  assert.equal(REQUEST_COUNT_LIMIT, 501);
  assert.equal(requestBadgeCount(ids(500), new Set()), 500);
  assert.equal(requestBadgeCount(ids(501), new Set()), undefined);
  assert.equal(requestBadgeCount([], new Set()), 0);
});

test("dismissed requests do not count, but the cap uses the full read", () => {
  assert.equal(requestBadgeCount(ids(3), new Set(["request-1"])), 2);
  assert.equal(requestBadgeCount(ids(501), new Set(["request-1"])), undefined);
});

test("paging and the detail window (conversationListRules)", () => {
  assert.deepEqual(detailWindow(10, 5, 100), [10, 65]);
  assert.deepEqual(detailWindow(-4, 5, 100), [0, 55]);
  assert.deepEqual(detailWindow(90, 5, 100), [90, 100]);
  assert.deepEqual(detailWindow(120, 5, 100), [100, 100]);
  assert.ok(shouldLoadNextPage(47, 50, ""));
  assert.ok(!shouldLoadNextPage(46, 50, ""));
  assert.ok(!shouldLoadNextPage(50, 50, "hello"));
});
