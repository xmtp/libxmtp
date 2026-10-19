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

test("round trips with the iOS key names", () => {
  const text = serializeSessionIndex(index);
  const json = JSON.parse(text) as Record<string, unknown>;
  assert.ok("activeID" in json);
  assert.deepEqual(
    (json["reset"] as Record<string, unknown>)["profileID"],
    "B",
  );
  assert.ok(text.includes('"inboxID":"inbox"'));
  assert.deepEqual(parseSessionIndex(text), index);
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
