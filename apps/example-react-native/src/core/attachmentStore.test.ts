import assert from "node:assert/strict";
import { test } from "node:test";

import {
  FakeGateway,
  FileConversation,
  MemoryPreferences,
  attachmentError,
  remoteFile,
} from "./attachmentFakes.ts";
import {
  AttachmentCopy,
  AttachmentDraftStore,
  AttachmentError,
  type FileDraft,
  VerifiedFileCache,
} from "./attachments.ts";
import {
  AttachmentStore,
  type AttachmentStorePorts,
} from "./attachmentStore.ts";
import { CONVERSATION, MemoryJournal, coordinatorFor } from "./chatFakes.ts";
import { SendError, SendJournal } from "./sendJournal.ts";
import { deferred, settle } from "./testing.ts";

function harness(
  options: {
    conversation?: FileConversation;
    preferences?: MemoryPreferences;
    journal?: MemoryJournal;
  } = {},
) {
  const conversation = options.conversation ?? new FileConversation();
  const gateway = new FakeGateway();
  const preferences = options.preferences ?? new MemoryPreferences();
  const drafts = new AttachmentDraftStore(preferences);
  const storage = options.journal ?? new MemoryJournal();
  const journal = new SendJournal(storage);
  const coordinator = coordinatorFor(conversation, journal);
  const removed: string[] = [];
  const env: {
    admitted: boolean;
    allowed: boolean;
    authorized: boolean;
    changes: number;
    authorize?: () => Promise<boolean>;
  } = { admitted: true, allowed: true, authorized: true, changes: 0 };
  let next = 0;
  const activeSends = new Set<string>();
  const ports = (conversationId?: string): AttachmentStorePorts => ({
    gateway,
    drafts,
    coordinator,
    ...(conversationId === undefined ? {} : { conversationId }),
    admit: () => env.admitted,
    acceptsSession: () => true,
    allowed: () => env.allowed,
    authorizeDownload: () =>
      env.authorize?.() ?? Promise.resolve(env.authorized),
    activeSends,
    sendFile: (id, file) =>
      conversation.queue(id, { kind: "attachment", file }),
    published: async (id, messageId) => {
      const row = await conversation.message(messageId);
      return row?.conversationId === id && row.delivery === "published";
    },
    removeStaged: (path) => {
      removed.push(path);
    },
    changed: () => {
      env.changes += 1;
      return Promise.resolve();
    },
    newId: () => {
      next += 1;
      return `D${String(next)}`;
    },
    now: () => 1,
  });
  const store = new AttachmentStore(ports(CONVERSATION));
  return {
    store,
    /** Attachment recovery's store in the same session. */
    recovery: () => new AttachmentStore(ports()),
    gateway,
    conversation,
    drafts,
    journal,
    storage,
    preferences,
    removed,
    env,
  };
}

async function staged(h: ReturnType<typeof harness>, name = "notes.txt") {
  await h.store.load();
  await h.store.stage(`/temp/attachment-staging/${name}`, name);
  const draft = h.store.getState().drafts.at(-1);
  assert.ok(draft);
  return draft;
}

test("staging hands the copy to the SDK, removes it, and saves a waiting draft", async () => {
  const h = harness();
  const draft = await staged(h);
  assert.deepEqual(h.gateway.created, [
    {
      path: "/temp/attachment-staging/notes.txt",
      filename: "notes.txt",
      mimeType: "text/plain",
    },
  ]);
  // An extension outside the table takes the picker's MIME type.
  await h.store.stage(
    "/temp/attachment-staging/raw",
    "raw.dng",
    "image/x-adobe-dng",
  );
  assert.equal(h.gateway.created[1]?.mimeType, "image/x-adobe-dng");
  assert.deepEqual(h.removed, [
    "/temp/attachment-staging/notes.txt",
    "/temp/attachment-staging/raw",
  ]);
  assert.equal(draft.phase, "waiting");
  assert.equal(draft.conversationId, CONVERSATION);
  assert.deepEqual(h.drafts.all()[0], draft);
});

test("a failed create shows its error, a server without attachments gets nothing, and both remove the staged copy", async () => {
  const h = harness();
  await h.store.load();
  h.gateway.createError = new AttachmentError(
    AttachmentCopy.unavailable,
    false,
  );
  await h.store.stage("/temp/x", "x.txt");
  assert.deepEqual(h.drafts.all(), []);
  assert.equal(h.store.getState().error, AttachmentCopy.unavailable);
  // The server stops offering attachments: a reload drops the limit.
  h.gateway.limit = undefined;
  await h.store.load();
  assert.equal(h.store.getState().limit, undefined);
  await h.store.stage("/temp/y", "y.txt");
  assert.equal(h.gateway.created.length, 1);
  assert.deepEqual(h.removed, ["/temp/x", "/temp/y"]);
});

test("send uploads before it queues, then publishes and removes the draft", async () => {
  const h = harness();
  const draft = await staged(h);
  let atQueue: unknown[] = [];
  h.conversation.onQueue = () => {
    atQueue = [
      h.gateway.uploads.length,
      h.journal.all()[0]?.phase,
      h.drafts.all()[0]?.phase,
    ];
  };
  await h.store.send(draft.id);
  // Uploaded, and the unknown outcome recorded, before the SDK call.
  assert.deepEqual(atQueue, [1, "queueing", "unknown"]);
  assert.equal(h.conversation.queued.length, 1);
  assert.equal(h.conversation.published.length, 1);
  assert.deepEqual(h.drafts.all(), []);
  assert.deepEqual(h.store.getState().drafts, []);
  assert.deepEqual(h.journal.all(), []);
  assert.ok(h.env.changes > 0);
});

test("an accepted draft whose publish failed retries without uploading again", async () => {
  const h = harness();
  const draft = await staged(h);
  h.conversation.publishError = new Error("offline");
  await h.store.send(draft.id);
  const accepted = h.drafts.all()[0];
  assert.equal(accepted?.phase, "complete");
  assert.notEqual(accepted?.acceptedId, undefined);
  h.conversation.publishError = undefined;
  await h.store.send(draft.id);
  assert.equal(h.gateway.uploads.length, 1);
  assert.equal(h.conversation.queued.length, 1);
  assert.deepEqual(h.drafts.all(), []);
});

test("a definite rejection restores the uploaded draft for a send without upload", async () => {
  const h = harness();
  const draft = await staged(h);
  h.conversation.queueError = new SendError("rejected");
  await h.store.send(draft.id);
  assert.equal(h.drafts.all()[0]?.phase, "complete");
  assert.deepEqual(h.journal.all(), []);
  await h.store.send(draft.id);
  assert.equal(h.gateway.uploads.length, 1);
  assert.deepEqual(h.drafts.all(), []);
});

test("an ambiguous queue failure stays unknown and cannot be queued again", async () => {
  const h = harness();
  const draft = await staged(h);
  h.conversation.queueError = new Error("connection reset");
  await h.store.send(draft.id);
  assert.equal(h.drafts.all()[0]?.phase, "unknown");
  assert.equal(h.journal.all()[0]?.phase, "queueing");
  await h.store.send(draft.id);
  assert.equal(h.gateway.uploads.length, 1);
  assert.equal(h.conversation.queued.length, 0);
});

/** Holds the next uploads until `gate` resolves. */
function holdUploads(h: ReturnType<typeof harness>) {
  const gate = deferred();
  const upload = h.gateway.upload.bind(h.gateway);
  h.gateway.upload = async (file) => {
    await gate.promise;
    return upload(file);
  };
  return gate;
}

test("upload failures follow the SDK retry rule, and a reload while the upload runs keeps it uploading", async () => {
  const h = harness();
  const draft = await staged(h);
  // The SDK did not finish the upload: nothing is queued and Retry is offered.
  h.gateway.uploadPhase = "uploading";
  await h.store.send(draft.id);
  assert.equal(h.conversation.queued.length, 0);
  assert.deepEqual(
    [h.drafts.all()[0]?.phase, h.drafts.all()[0]?.retryable],
    ["failed", true],
  );
  // Retry uploads again; the chat regains focus and a full load runs meanwhile.
  h.gateway.uploadPhase = "complete";
  const gate = holdUploads(h);
  const sending = h.store.send(draft.id);
  await settle();
  h.store.reloadDrafts();
  assert.equal(h.store.getState().drafts[0]?.phase, "uploading");
  await h.store.load();
  assert.equal(h.store.getState().drafts[0]?.phase, "uploading");
  assert.equal(h.drafts.all()[0]?.phase, "uploading");
  h.gateway.uploadError = attachmentError(false);
  gate.resolve();
  await sending;
  assert.deepEqual(
    [h.drafts.all()[0]?.phase, h.drafts.all()[0]?.retryable],
    ["failed", false],
  );
  assert.equal(h.store.getState().drafts[0]?.phase, "failed");
  // A terminal failure cannot be sent again.
  await h.store.send(draft.id);
  assert.equal(h.gateway.uploads.length, 2);
});

test("recovery opened during a chat's upload keeps it uploading and cannot send it again", async () => {
  const h = harness();
  const draft = await staged(h);
  // The chat's send uploads, then its queue call fails with an unknown outcome.
  h.conversation.queueError = new Error("connection reset");
  const gate = holdUploads(h);
  const sending = h.store.send(draft.id);
  await settle();
  const recovery = h.recovery();
  await recovery.load();
  assert.equal(recovery.getState().drafts[0]?.phase, "uploading");
  const second = recovery.send(draft.id);
  gate.resolve();
  await Promise.all([sending, second]);
  assert.equal(h.gateway.uploads.length, 1);
  // The unknown outcome stays: nothing can queue the file again.
  assert.equal(h.drafts.all()[0]?.phase, "unknown");
  assert.equal(h.conversation.queued.length, 0);
});

test("a discard during the upload keeps the send from queuing, and a draft whose SDK delete failed can still be sent", async () => {
  const h = harness();
  const gone = await staged(h, "gone.txt");
  const kept = await staged(h, "kept.txt");
  const gate = holdUploads(h);
  const discard = h.gateway.discard.bind(h.gateway);
  h.gateway.discard = (file) =>
    file.filename === kept.file.filename
      ? Promise.reject(attachmentError(true))
      : discard(file);
  const sends = [h.store.send(gone.id), h.store.send(kept.id)];
  await settle();
  await h.store.discard(gone.id);
  await h.store.discard(kept.id);
  gate.resolve();
  await Promise.all(sends);
  assert.equal(h.conversation.queued.length, 0);
  assert.deepEqual(
    h.gateway.discarded.map((file) => file.filename),
    ["gone.txt"],
  );
  // The draft whose SDK delete failed stays for another try.
  assert.deepEqual(
    h.drafts.all().map((draft) => draft.id),
    [kept.id],
  );
  await h.store.send(kept.id);
  assert.equal(h.conversation.queued.length, 1);
});

test("a focus reload reads drafts other screens saved and imports nothing", async () => {
  const h = harness();
  await h.store.load();
  // Recovery assigned a draft to this chat; the SDK also holds an orphan.
  h.drafts.save({
    id: "R",
    conversationId: CONVERSATION,
    file: remoteFile("assigned.pdf"),
    phase: "waiting",
    retryable: true,
  });
  h.gateway.pendingFiles = [
    remoteFile("assigned.pdf"),
    remoteFile("orphan.pdf"),
  ];
  h.store.reloadDrafts();
  await settle();
  assert.deepEqual(
    h.store.getState().drafts.map((draft) => draft.file.filename),
    ["assigned.pdf"],
  );
  assert.equal(h.drafts.all().length, 1);
});

test("load imports SDK pending uploads as unassigned drafts, skips saved and discarded files, and resets interrupted uploads", async () => {
  const preferences = new MemoryPreferences();
  const saved: FileDraft = {
    id: "S",
    conversationId: CONVERSATION,
    file: remoteFile("photo.jpg"),
    phase: "uploading",
    retryable: true,
  };
  new AttachmentDraftStore(preferences).save(saved);
  // The chat is open, and its load still assigns an import to no chat.
  const h = harness({ preferences });
  h.gateway.pendingFiles = [remoteFile("photo.jpg"), remoteFile("orphan.pdf")];
  await h.store.load();
  const drafts = h.store.getState().drafts;
  assert.deepEqual(
    drafts.map((draft) => [
      draft.file.filename,
      draft.conversationId,
      draft.phase,
    ]),
    [
      ["photo.jpg", CONVERSATION, "waiting"],
      ["orphan.pdf", undefined, "waiting"],
    ],
  );
  h.store.assign(drafts[1]?.id ?? "", "other-chat");
  assert.equal(h.drafts.all()[1]?.conversationId, "other-chat");
  // The SDK deleted its copy, but its pending list still names the file.
  h.gateway.discard = (file) => {
    h.gateway.discarded.push(file);
    return Promise.resolve();
  };
  await h.store.discard("S");
  await h.store.load();
  assert.deepEqual(
    h.store.getState().drafts.map((draft) => draft.file.filename),
    ["orphan.pdf"],
  );
  assert.equal(h.drafts.all().length, 1);
});

test("load adopts a journal-accepted send and removes it once published", async () => {
  const preferences = new MemoryPreferences();
  const journal = new MemoryJournal();
  const conversation = new FileConversation();
  const file = remoteFile("a.txt");
  const messageId = await conversation.queue(CONVERSATION, {
    kind: "attachment",
    file,
  });
  journal.drafts = [
    {
      id: "S",
      conversationId: CONVERSATION,
      phase: "accepted",
      messageId,
      createdAt: 1,
    },
  ];
  new AttachmentDraftStore(preferences).save({
    id: "S",
    conversationId: CONVERSATION,
    file,
    phase: "unknown",
    retryable: true,
  });
  const h = harness({ preferences, journal, conversation });
  await h.store.load();
  assert.deepEqual(
    h.drafts.all().map((draft) => [draft.phase, draft.acceptedId]),
    [["complete", messageId]],
  );
  await conversation.publish(CONVERSATION, messageId);
  await h.store.load();
  assert.deepEqual(h.drafts.all(), []);
});

test("downloads re-check consent, run once per message, and verify", async () => {
  const h = harness();
  const file = remoteFile("report.pdf");
  h.env.authorized = false;
  await h.store.download("m1", file);
  assert.equal(h.gateway.downloads.length, 0);
  assert.equal(h.store.card("m1", file).phase, "available");
  // A second tap whose consent check would end after the first download.
  const checks: { resolve: (allowed: boolean) => void }[] = [];
  h.env.authorize = () => {
    const check = deferred<boolean>();
    checks.push(check);
    return check.promise;
  };
  const first = h.store.download("m1", file);
  const second = h.store.download("m1", file);
  await settle();
  checks[0]?.resolve(true);
  await first;
  checks[1]?.resolve(true);
  await second;
  assert.equal(h.gateway.downloads.length, 1);
  assert.equal(h.store.card("m1", file).phase, "verified");
  assert.equal(h.store.verified("m1")?.filename, "report.pdf");
});

test("a download failure shows Failed with the SDK copy and retry rule", async () => {
  const h = harness();
  const file = remoteFile("report.pdf");
  h.gateway.downloadError = new AttachmentError(
    AttachmentCopy.unverified,
    false,
  );
  await h.store.download("m1", file);
  const card = h.store.card("m1", file);
  assert.deepEqual(
    [card.phase, card.retryable, card.error],
    ["failed", false, AttachmentCopy.unverified],
  );
});

test("a consent change drops verified files and a running download's result", async () => {
  const h = harness();
  const file = remoteFile("report.pdf");
  await h.store.download("m1", file);
  const gate = deferred();
  h.gateway.downloadGate = gate.promise;
  const running = h.store.download("m2", file);
  await settle();
  assert.equal(h.store.card("m2", file).phase, "downloading");
  h.store.invalidateDownloads();
  gate.resolve();
  await running;
  assert.equal(h.store.card("m1", file).phase, "available");
  assert.equal(h.store.verified("m1"), undefined);
  assert.equal(h.store.card("m2", file).phase, "available");
  assert.equal(h.store.verified("m2"), undefined);
});

test("a closed conversation cannot download or send, and a closed chat stops its send", async () => {
  const h = harness();
  const draft = await staged(h);
  h.env.allowed = false;
  await h.store.download("m1", remoteFile());
  await h.store.send(draft.id);
  assert.equal(h.gateway.downloads.length, 0);
  assert.equal(h.gateway.uploads.length, 0);
  // The chat closes while the upload runs: nothing is queued, the draft stays.
  h.env.allowed = true;
  const gate = holdUploads(h);
  const sending = h.store.send(draft.id);
  await settle();
  h.env.admitted = false;
  gate.resolve();
  await sending;
  assert.equal(h.conversation.queued.length, 0);
  assert.equal(h.drafts.all()[0]?.id, draft.id);
});

test("a file pushed out of the verified cache shows Download again", async () => {
  const h = harness();
  const file = remoteFile("report.pdf");
  for (let n = 0; n < VerifiedFileCache.LIMIT; n += 1) {
    await h.store.download(`m${String(n)}`, file);
  }
  // Opening m0 keeps it; m1 is now the least recently used.
  assert.notEqual(h.store.verified("m0"), undefined);
  await h.store.download("newest", file);
  assert.equal(h.store.card("m1", file).phase, "available");
  assert.equal(h.store.verified("m1"), undefined);
  assert.equal(h.store.card("m0", file).phase, "verified");
  const verified = Object.values(h.store.getState().cards).filter(
    (card) => card.phase === "verified",
  );
  assert.equal(verified.length, VerifiedFileCache.LIMIT);
});
