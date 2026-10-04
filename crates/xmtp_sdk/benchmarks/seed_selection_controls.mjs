import assert from "node:assert/strict";

import { seedRows } from "./hosts/workload.mjs";

const messages = Array.from({ length: 10_000 }, (_, index) => ({
  key: String(index),
  reply_to: index % 4 === 1 ? String(index - 1) : null,
  reactions: index % 4 === 3 ? [{ content: "+1" }] : [],
}));
const fixture = {
  messages,
  page_keys: messages.slice(0, 1_000).map((row) => row.key),
  stream_keys: messages.map((row) => row.key),
};

for (const [stream, expectedRows, expectedReactions] of [
  [false, 1_000, 250],
  [true, 10_000, 2_500],
]) {
  const rows = seedRows(fixture, stream);
  assert.equal(rows.length, expectedRows);
  assert.deepEqual(
    rows.map((row) => row.key),
    stream ? fixture.stream_keys : fixture.page_keys,
  );
  assert.equal(
    rows.reduce((count, row) => count + row.reactions.length, 0),
    expectedReactions,
  );
  const seeded = new Set(rows.map((row) => row.key));
  assert.ok(
    rows.every((row) => row.reply_to === null || seeded.has(row.reply_to)),
  );
}

const browserStream = seedRows(
  { ...fixture, stream_keys: fixture.stream_keys.slice(0, 500) },
  true,
);
assert.equal(browserStream.length, 500);
assert.equal(
  browserStream.reduce((count, row) => count + row.reactions.length, 0),
  125,
);
assert.equal(
  browserStream.reduce((count, row) => count + 1 + row.reactions.length, 0),
  625,
);

assert.throws(
  () => seedRows({ ...fixture, page_keys: ["1"] }, false),
  /Invalid benchmark seed row 1/,
);

console.log(
  "Page seeds 1,000 primary and 250 reactions; stream seeds 10,000/2,500, or Browser 500/125.",
);
