import assert from "node:assert/strict";
import { test } from "node:test";

import {
  CorruptIndexError,
  EMPTY_SESSION_INDEX,
  type SessionIndex,
  activeProfile,
  isEstablished,
  parseSessionIndex,
  profileForUrl,
  serializeSessionIndex,
  withProfile,
} from "./sessionIndex.ts";

const index: SessionIndex = {
  profiles: [
    {
      id: "A",
      url: "http://127.0.0.1:5150",
      inboxId: "inbox",
      identity: "0xabc",
    },
    { id: "B", url: "HTTPS://EXAMPLE.COM:443/API/v1/" },
  ],
  activeId: "A",
  signedIn: true,
  reset: { profileId: "B", phase: "DATABASE_REMOVED" },
  appearance: "dark",
};

test("the saved index format stays readable by later builds", () => {
  // Installed apps hold this exact JSON; a change needs a migration.
  const saved =
    '{"profiles":[{"id":"A","url":"http://127.0.0.1:5150","inboxID":"inbox","identity":"0xabc"},' +
    '{"id":"B","url":"HTTPS://EXAMPLE.COM:443/API/v1/"}],"activeID":"A","signedIn":true,' +
    '"reset":{"profileID":"B","phase":"DATABASE_REMOVED"},"appearance":"dark"}';
  assert.equal(serializeSessionIndex(index), saved);
  assert.deepEqual(parseSessionIndex(saved), index);
  // An index saved during a reset reads back in every phase.
  for (const phase of [
    "STOPPING",
    "DATABASE_REMOVED",
    "FILES_REMOVED",
  ] as const) {
    const resetting: SessionIndex = {
      ...index,
      reset: { profileId: "B", phase },
    };
    assert.deepEqual(
      parseSessionIndex(serializeSessionIndex(resetting)),
      resetting,
      phase,
    );
  }
  assert.deepEqual(
    parseSessionIndex(serializeSessionIndex(EMPTY_SESSION_INDEX)),
    EMPTY_SESSION_INDEX,
  );
});

test("missing fields take the defaults; unknown appearance is system", () => {
  assert.deepEqual(parseSessionIndex("{}"), EMPTY_SESSION_INDEX);
  assert.equal(parseSessionIndex('{"appearance":"blue"}').appearance, "system");
});

test("a malformed index throws instead of reading as empty", () => {
  for (const text of [
    "",
    "[]",
    "not json",
    '{"profiles":{}}',
    '{"signedIn":"yes"}',
    '{"profiles":[{"url":"x"}]}',
    '{"reset":{"profileID":"A","phase":"DONE"}}',
    '{"activeID":5}',
  ]) {
    assert.throws(() => parseSessionIndex(text), CorruptIndexError, text);
  }
});

test("profiles match by normalized URL, also for legacy stored URLs", () => {
  assert.equal(profileForUrl(index, "https://example.com/API/v1")?.id, "B");
  assert.equal(profileForUrl(index, "http://127.0.0.1:5150")?.id, "A");
  assert.equal(profileForUrl(index, "https://example.com/api/v1"), undefined);
  assert.equal(activeProfile(index)?.id, "A");
  assert.equal(activeProfile({ ...index, activeId: undefined }), undefined);
});

test("a profile with an inbox or identity is established; withProfile replaces its entry", () => {
  assert.ok(isEstablished({ id: "x", url: "u", inboxId: "i" }));
  assert.ok(isEstablished({ id: "x", url: "u", identity: "0x" }));
  assert.ok(!isEstablished({ id: "x", url: "u" }));
  const next = withProfile(index, {
    id: "A",
    url: "http://127.0.0.1:5150",
    inboxId: "new",
  });
  assert.equal(next.profiles.length, 2);
  assert.deepEqual(
    next.profiles.filter((profile) => profile.id === "A"),
    [{ id: "A", url: "http://127.0.0.1:5150", inboxId: "new" }],
  );
});
