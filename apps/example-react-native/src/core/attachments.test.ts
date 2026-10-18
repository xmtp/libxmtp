import assert from "node:assert/strict";
import { test } from "node:test";

import { MemoryPreferences, remoteFile } from "./attachmentFakes.ts";
import {
  AttachmentCopy,
  AttachmentDraftStore,
  type FileDraft,
  VerifiedFileCache,
  attachmentFailure,
  byteCount,
  cardStatus,
  draftMetadata,
  exceedsLimit,
  fileTypeDescription,
  isVisualFile,
  mimeTypeFor,
  parseDraft,
  encodeDraft,
  photoName,
  safeFileName,
} from "./attachments.ts";

test("SDK failure causes map to the iOS copy and retry rule", () => {
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

test("byte counts read like ByteCountFormatter's file style", () => {
  assert.equal(byteCount(0), "Zero KB");
  assert.equal(byteCount(1), "1 byte");
  assert.equal(byteCount(12), "12 bytes");
  assert.equal(byteCount(999), "999 bytes");
  assert.equal(byteCount(1000), "1 KB");
  assert.equal(byteCount(17_400), "17 KB");
  assert.equal(byteCount(212_345), "212 KB");
  assert.equal(byteCount(1_234_567), "1.2 MB");
  assert.equal(byteCount(2_000_000), "2 MB");
  assert.equal(byteCount(3_456_000_000), "3.46 GB");
});

test("card status follows consent first, then the phase", () => {
  assert.equal(cardStatus("available", false), AttachmentCopy.closed);
  assert.equal(cardStatus("verified", false), AttachmentCopy.closed);
  assert.equal(cardStatus("available", true), "Download to verify");
  assert.equal(cardStatus("downloading", true), "Downloading");
  assert.equal(cardStatus("verified", true), "Verified");
  assert.equal(cardStatus("failed", true), "Failed");
  assert.equal(cardStatus("expired", true), "Expired");
});

test("names, types and the staging limit", () => {
  assert.equal(mimeTypeFor("photo.JPG"), "image/jpeg");
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
});

test("type lines and MIME types read like UTType", () => {
  // UTType values on the iOS 27 simulator.
  assert.equal(fileTypeDescription("l6-proof.txt"), "text");
  assert.equal(fileTypeDescription("IMG_0001.HEIC"), "HEIF Image");
  assert.equal(fileTypeDescription("sticker.webp"), "WebP Image");
  assert.equal(fileTypeDescription("photos.zip"), "Zip archive");
  assert.equal(fileTypeDescription("budget.csv"), "comma-separated values");
  assert.equal(
    fileTypeDescription("Report.docx"),
    "Office Open XML word processing document",
  );
  assert.equal(fileTypeDescription("scan.tiff"), "TIFF image");
  assert.equal(fileTypeDescription("notes.unknownext"), "File");
  assert.equal(mimeTypeFor("scan.tif"), "image/tiff");
  assert.equal(mimeTypeFor("voice.m4a"), "audio/x-m4a");
  // The picker's type covers extensions outside the table.
  assert.equal(
    mimeTypeFor("raw.dng", "image/x-adobe-dng"),
    "image/x-adobe-dng",
  );
});

test("a saved draft round-trips with its key bytes", () => {
  const draft: FileDraft = {
    id: "A",
    conversationId: "chat",
    file: remoteFile(),
    phase: "complete",
    acceptedId: "m1",
    retryable: false,
  };
  const parsed = parseDraft(JSON.parse(JSON.stringify(encodeDraft(draft))));
  assert.deepEqual(parsed, draft);
  assert.throws(() => parseDraft({ id: "A", phase: "nope" }));
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
  preferences.writeJson("attachment-3", encodeDraft({ ...first, id: "3" }));
  preferences.remove("attachment-2");
  store.cleanOrphans();
  assert.equal(preferences.readJson("attachment-3"), undefined);
  assert.deepEqual(preferences.readJson("attachment-refs"), ["attachment-1"]);
  store.remove("1");
  assert.deepEqual(store.all(), []);
});

test("the verified cache keeps the 16 most recently used files", () => {
  const cache = new VerifiedFileCache();
  const file = { path: "/p", filename: "f" };
  for (let index = 0; index < 16; index += 1) {
    assert.deepEqual(cache.put(`m${String(index)}`, file), []);
  }
  cache.get("m0");
  assert.deepEqual(cache.put("m16", file), ["m1"]);
  assert.notEqual(cache.get("m0"), undefined);
  assert.equal(cache.get("m1"), undefined);
});
