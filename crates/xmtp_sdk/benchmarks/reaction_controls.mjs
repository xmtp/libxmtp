import assert from "node:assert/strict";

import { enrichLive } from "./hosts/live.mjs";
const a = { content: "+1", schema: "unicode", action: "added" };
const b = { content: "ok", schema: "unicode", action: "added" };
const primary = { id: "p", kind: "text", text: "body" };
const reaction = (id, value) => ({
  id,
  kind: "reaction",
  reference: "p",
  reaction: value,
});
const records = [];
for (const [fault, eager, delivered, fail] of [
  ["exact", [a, b, a], [a, b, a], false],
  ["reordered", [b, a, a], [a, b, a], false],
  ["extra_eager", [a, b], [a], true],
  ["duplicate_overcount", [a, a], [a], true],
  ["schema", [{ ...a, schema: "shortcode" }], [a], true],
  ["action", [{ ...a, action: "removed" }], [a], true],
  ["future_reaction", [], [a], false],
  ["unavailable_eager", undefined, [a], false],
]) {
  const events = [
    { ...primary, eager_reactions: eager },
    ...delivered.map((value, index) => reaction(`r${index}`, value)),
  ];
  let failure;
  try {
    enrichLive(events, ["p"]);
  } catch (error) {
    failure = String(error);
  }
  records.push({
    fault,
    rejected: Boolean(failure),
    expected: fail,
    failure: failure ?? null,
  });
  assert.equal(Boolean(failure), fail, JSON.stringify(records));
}
// Stored eager reactions may arrive after the primary item at the app boundary.
const events = [
  reaction("before", a),
  { ...primary, eager_reactions: [a, b] },
  reaction("after", b),
];
assert.equal(enrichLive(events, ["p"])[0].reactions.length, 2);
console.log(
  JSON.stringify(
    {
      records,
      snapshot_completeness:
        "PENDING: neither arrival order nor final delivery is the DB snapshot",
    },
    null,
    2,
  ),
);
