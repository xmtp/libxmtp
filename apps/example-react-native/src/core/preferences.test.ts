import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { test } from "node:test";

import {
  type PreferenceFiles,
  PreferenceError,
  ProfilePreferences,
  composerDraftName,
  isPreferenceName,
} from "./preferences.ts";
import { SendJournal } from "./sendJournal.ts";

class MemoryFiles implements PreferenceFiles {
  readonly files = new Map<string, string>();
  reads = 0;
  failWrites = false;

  read(name: string): string | undefined {
    this.reads += 1;
    return this.files.get(name);
  }

  write(name: string, text: string): void {
    if (this.failWrites) throw new Error("disk full");
    this.files.set(name, text);
  }

  remove(name: string): void {
    this.files.delete(name);
  }

  list(): readonly string[] {
    return [...this.files.keys()];
  }
}

test("feature preferences persist across instances with defaults when absent", () => {
  const files = new MemoryFiles();
  const prefs = new ProfilePreferences(files);
  assert.equal(prefs.serverLabel, undefined);
  assert.equal(prefs.skippedHandle, false);
  assert.equal(prefs.dismissedRequests.size, 0);
  prefs.setServerLabel("Local");
  prefs.setAlias("c1", "Family");
  prefs.setAlias("c2", "  ");
  prefs.dismissRequest("r1");
  prefs.dismissRequest("r1");
  prefs.skipHandle();
  prefs.recordClaim("nick", new Date(1_000));
  const reopened = new ProfilePreferences(files);
  assert.equal(reopened.serverLabel, "Local");
  assert.equal(reopened.alias("c1"), "Family");
  assert.equal(reopened.alias("c2"), undefined);
  assert.deepEqual([...reopened.dismissedRequests], ["r1"]);
  assert.equal(reopened.skippedHandle, true);
  assert.deepEqual(reopened.claimTimes().get("nick"), new Date(1_000));
  reopened.setAlias("c1", undefined);
  assert.equal(new ProfilePreferences(files).alias("c1"), undefined);
  assert.ok(files.files.has("feature-preferences"));
});

test("read markers store bigints as decimal strings", () => {
  const files = new MemoryFiles();
  const prefs = new ProfilePreferences(files);
  assert.equal(prefs.marker("dm:a:b"), 0n);
  prefs.setMarker("dm:a:b", 1_760_000_000_123_456_789n);
  assert.equal(
    new ProfilePreferences(files).marker("dm:a:b"),
    1_760_000_000_123_456_789n,
  );
  assert.match(files.files.get("read-markers") ?? "", /"1760000000123456789"/u);
});

test("composer drafts use the sha256 file name and parse strictly", () => {
  const files = new MemoryFiles();
  const prefs = new ProfilePreferences(files);
  const name = composerDraftName("conversation-1");
  assert.equal(
    name,
    `composer-${createHash("sha256").update("conversation-1").digest("hex")}`,
  );
  prefs.saveComposerDraft("conversation-1", {
    text: "hi",
    revision: 2,
    replyId: "m1",
  });
  assert.deepEqual(
    new ProfilePreferences(files).composerDraft("conversation-1"),
    {
      text: "hi",
      revision: 2,
      replyId: "m1",
    },
  );
  prefs.removeComposerDraft("conversation-1");
  assert.equal(prefs.composerDraft("conversation-1"), undefined);
  files.files.set(name, '{"text":3}');
  assert.throws(
    () => new ProfilePreferences(files).composerDraft("conversation-1"),
    PreferenceError,
  );
});

test("scroll anchors and the send journal round trip", () => {
  const files = new MemoryFiles();
  const prefs = new ProfilePreferences(files);
  prefs.setScrollAnchor("c", { messageId: "m", sentAtNs: 99n, cursor: "k" });
  prefs.setScrollAnchor("d", { messageId: "n", sentAtNs: 1n, cursor: "j" });
  prefs.setScrollAnchor("d", undefined);
  const reopened = new ProfilePreferences(files);
  assert.deepEqual(reopened.scrollAnchor("c"), {
    messageId: "m",
    sentAtNs: 99n,
    cursor: "k",
  });
  assert.equal(reopened.scrollAnchor("d"), undefined);
  const journal = new SendJournal(reopened.sendJournalStorage());
  journal.save({
    id: "r",
    conversationId: "c",
    phase: "queueing",
    createdAt: 3,
  });
  assert.deepEqual(
    new SendJournal(new ProfilePreferences(files).sendJournalStorage()).all(),
    [{ id: "r", conversationId: "c", phase: "queueing", createdAt: 3 }],
  );
});

test("a failed write keeps the cached value and throws", () => {
  const files = new MemoryFiles();
  const prefs = new ProfilePreferences(files);
  prefs.setServerLabel("first");
  files.failWrites = true;
  assert.throws(() => {
    prefs.setServerLabel("second");
  }, /disk full/u);
  assert.equal(prefs.serverLabel, "first");
});

test("reads are cached and corrupt files throw", () => {
  const files = new MemoryFiles();
  files.files.set("feature-preferences", "{not json");
  const prefs = new ProfilePreferences(files);
  assert.throws(() => prefs.serverLabel, PreferenceError);
  files.files.set("read-markers", '{"k":"1"}');
  const fresh = new ProfilePreferences(files);
  fresh.marker("k");
  fresh.marker("k");
  assert.equal(files.reads, 2);
  files.files.set("read-markers", '{"k":"x"}');
  assert.throws(
    () => new ProfilePreferences(files).marker("k"),
    PreferenceError,
  );
});

test("names cannot escape the prefs directory", () => {
  for (const name of ["", ".", "..", "a/b", "../x"]) {
    assert.ok(!isPreferenceName(name), name);
    assert.throws(
      () => new ProfilePreferences(new MemoryFiles()).readJson(name),
      PreferenceError,
    );
  }
  assert.ok(isPreferenceName("attachment-1"));
  const files = new MemoryFiles();
  const prefs = new ProfilePreferences(files);
  prefs.writeJson("attachment-1", { a: 1 });
  prefs.writeJson("attachment-2", { a: 2 });
  prefs.writeJson("other", {});
  assert.deepEqual([...prefs.names("attachment-")].sort(), [
    "attachment-1",
    "attachment-2",
  ]);
});
