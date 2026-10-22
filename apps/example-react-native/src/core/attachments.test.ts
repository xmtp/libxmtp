import assert from "node:assert/strict";
import { test } from "node:test";

import { MemoryPreferences, remoteFile } from "./attachmentFakes.ts";
import {
  AttachmentCopy,
  AttachmentDraftStore,
  type FileDraft,
  attachmentFailure,
  byteCount,
  cardStatus,
  draftMetadata,
  exceedsLimit,
  isVisualFile,
  mimeTypeFor,
  photoName,
  safeFileName,
} from "./attachments.ts";

test("SDK failure causes map to their message and retry rule", () => {
  const cases: [string, string, boolean, number?][] = [
    ["notOffered", AttachmentCopy.notOffered, false],
    ["tooLarge", AttachmentCopy.sizeLimit, false],
    ["stagedUnusable", AttachmentCopy.unavailable, false],
    ["deleted", AttachmentCopy.unavailable, true],
    ["notFound", AttachmentCopy.notFound, true],
    ["digestMismatch", AttachmentCopy.unverified, false],
    ["blockedAddress", AttachmentCopy.notAllowed, false],
    ["network", AttachmentCopy.operationFailed, true],
    ["httpStatus", AttachmentCopy.operationFailed, true, 503],
    ["httpStatus", AttachmentCopy.operationFailed, true, 429],
    ["httpStatus", AttachmentCopy.operationFailed, false, 404],
    ["connectionBlocked", AttachmentCopy.operationFailed, false],
  ];
  for (const [cause, message, retryable, httpStatus] of cases) {
    const error = attachmentFailure({ cause, retryable: false, httpStatus });
    assert.equal(error.message, message, cause);
    assert.equal(error.retryable, retryable, `${cause} ${String(httpStatus)}`);
  }
  const credential = attachmentFailure({
    cause: "credential",
    retryable: true,
  });
  assert.equal(credential.message, AttachmentCopy.refused);
  assert.equal(credential.retryable, true);
  // A thrown error's own retry detail wins.
  assert.equal(
    attachmentFailure({ cause: "network", retryable: false }, false).retryable,
    false,
  );
});

test("byte counts use decimal units: whole bytes and KB, one MB digit, two GB digits", () => {
  assert.equal(byteCount(0), "Zero KB");
  assert.equal(byteCount(1), "1 byte");
  assert.equal(byteCount(12), "12 bytes");
  assert.equal(byteCount(999), "999 bytes");
  assert.equal(byteCount(1000), "1 KB");
  // Whole units keep their zeroes; only a fraction's trailing zeroes go.
  assert.equal(byteCount(10_000), "10 KB");
  assert.equal(byteCount(100_000), "100 KB");
  assert.equal(byteCount(10_000_000), "10 MB");
  assert.equal(byteCount(17_400), "17 KB");
  assert.equal(byteCount(212_345), "212 KB");
  assert.equal(byteCount(1_234_567), "1.2 MB");
  assert.equal(byteCount(2_000_000), "2 MB");
  assert.equal(byteCount(3_456_000_000), "3.46 GB");
});

test("card status follows consent first, then the phase", () => {
  assert.equal(cardStatus("available", false), AttachmentCopy.closed);
  assert.equal(cardStatus("verified", false), AttachmentCopy.closed);
  assert.equal(cardStatus("available", true), AttachmentCopy.available);
  assert.equal(cardStatus("downloading", true), AttachmentCopy.downloading);
  assert.equal(cardStatus("verified", true), AttachmentCopy.verified);
  assert.equal(cardStatus("failed", true), AttachmentCopy.failed);
});

test("names, types and the staging limit", () => {
  // Types come from the last extension, in any case, else the picker's type.
  assert.equal(mimeTypeFor("photo.JPG"), "image/jpeg");
  assert.equal(mimeTypeFor("notes.v2.txt"), "text/plain");
  assert.equal(
    mimeTypeFor("archive.xyz", "application/x-xyz"),
    "application/x-xyz",
  );
  assert.equal(mimeTypeFor("noext"), "application/octet-stream");
  assert.equal(photoName("coastline.jpeg"), "coastline.jpeg");
  assert.equal(photoName(null, "image/png"), "Photo.png");
  assert.equal(photoName(undefined), "Photo.jpg");
  assert.equal(photoName("IMG", "video/quicktime"), "Photo.mov");
  assert.equal(safeFileName("../../etc/passwd"), "passwd");
  assert.equal(safeFileName("dir/"), "File");
  assert.equal(isVisualFile("Big Sur coastline.jpg"), true);
  assert.equal(isVisualFile("clip.MP4"), true);
  assert.equal(isVisualFile("Weekend itinerary.pdf"), false);
  assert.equal(isVisualFile(undefined), false);
  assert.equal(exceedsLimit(100, 100n), false);
  assert.equal(exceedsLimit(101, 100n), true);
  const draft: FileDraft = {
    id: "D",
    file: remoteFile("coastline.jpeg", 212_000),
    phase: "waiting",
    retryable: true,
  };
  assert.equal(draftMetadata(draft), "JPEG image · 212 KB");
  // An unknown extension reads as a plain file.
  assert.equal(
    draftMetadata({ ...draft, file: remoteFile("notes.unknownext", 12) }),
    `${AttachmentCopy.file} · 12 bytes`,
  );
});

/** A saved draft file as installed apps hold it. */
const SAVED_DRAFT = {
  id: "A",
  conversationId: "chat",
  phase: "complete",
  acceptedId: "m1",
  retryable: false,
  file: {
    url: "https://files.example/notes.txt",
    contentDigest: "digest-notes.txt",
    secret: "010203",
    salt: "0405",
    nonce: "06",
    scheme: "https://",
    contentLength: 12,
    filename: "notes.txt",
  },
};

test("saved drafts stay readable by later builds", () => {
  const draft: FileDraft = {
    id: "A",
    conversationId: "chat",
    file: remoteFile(),
    phase: "complete",
    acceptedId: "m1",
    retryable: false,
  };
  const written = new MemoryPreferences();
  new AttachmentDraftStore(written).save(draft);
  assert.deepEqual(written.readJson("attachment-A"), SAVED_DRAFT);
  assert.deepEqual(written.readJson("attachment-refs"), ["attachment-A"]);
  const installed = new MemoryPreferences();
  installed.writeJson("attachment-refs", ["attachment-A"]);
  installed.writeJson("attachment-A", SAVED_DRAFT);
  assert.deepEqual(new AttachmentDraftStore(installed).all(), [draft]);
  // A phase this build does not know makes the file malformed.
  installed.writeJson("attachment-A", { ...SAVED_DRAFT, phase: "nope" });
  assert.throws(() => new AttachmentDraftStore(installed).all());
});

test("the draft store lists saved drafts and drops orphans", () => {
  const preferences = new MemoryPreferences();
  const store = new AttachmentDraftStore(preferences);
  const first: FileDraft = {
    id: "1",
    file: remoteFile("a.txt"),
    phase: "waiting",
    retryable: true,
  };
  store.save(first);
  store.save({ ...first, id: "2", file: remoteFile("b.txt") });
  store.save({ ...first, phase: "uploading" });
  assert.deepEqual(
    store.all().map((draft) => [draft.id, draft.phase]),
    [
      ["1", "uploading"],
      ["2", "waiting"],
    ],
  );
  // An unlisted file and a listed name without a file.
  preferences.writeJson("attachment-3", preferences.readJson("attachment-1"));
  preferences.remove("attachment-2");
  store.cleanOrphans();
  assert.equal(preferences.readJson("attachment-3"), undefined);
  assert.deepEqual(preferences.readJson("attachment-refs"), ["attachment-1"]);
  store.remove("1");
  assert.deepEqual(store.all(), []);
});
