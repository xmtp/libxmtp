import assert from "node:assert/strict";
import { test } from "node:test";

import {
  FakeGateway,
  FileConversation,
  MemoryPreferences,
  attachmentError,
  deferred,
  remoteFile,
} from "./attachmentFakes.ts";
import { AttachmentDraftStore, type FileDraft } from "./attachments.ts";
import { AttachmentStore } from "./attachmentStore.ts";
import { CONVERSATION, MemoryJournal } from "./chatFakes.ts";
import { SendCoordinator, SendError, SendJournal } from "./sendJournal.ts";

function harness(
  options: {
    conversationId?: string | null;
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
  const coordinator = new SendCoordinator(
    journal,
    {
      consent: async (id) => (await conversation.conversation(id)).consent,
      message: async (id) => {
        const row = await conversation.message(id);
        return row === undefined
          ? undefined
          : {
              conversationId: row.conversationId,
              published: row.delivery === "published",
            };
      },
      publish: (conversationId, messageId) =>
        conversation.publish(conversationId, messageId),
    },
    () => true,
  );
  const removed: string[] = [];
  const env: {
    admitted: boolean;
    allowed: boolean;
    authorized: boolean;
    changes: number;
    authorize?: () => Promise<boolean>;
  } = { admitted: true, allowed: true, authorized: true, changes: 0 };
  let next = 0;
  const conversationId =
    options.conversationId === null
      ? undefined
      : (options.conversationId ?? CONVERSATION);
  const store = new AttachmentStore({
    gateway,
    drafts,
    coordinator,
    ...(conversationId === undefined ? {} : { conversationId }),
    admit: () => env.admitted,
    acceptsSession: () => true,
    allowed: () => env.allowed,
    authorizeDownload: () =>
      env.authorize?.() ?? Promise.resolve(env.authorized),
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
  return {
    store,
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

test("a failed create still removes the staged copy and shows the error", async () => {
  const h = harness();
  await h.store.load();
  h.gateway.createError = attachmentError(false);
  await h.store.stage("/temp/x", "x.txt");
  assert.deepEqual(h.removed, ["/temp/x"]);
  assert.deepEqual(h.drafts.all(), []);
  assert.equal(
    h.store.getState().error,
    "The file operation failed. Try again.",
  );
});

test("staging needs a server limit: without attachments nothing reaches the SDK", async () => {
  const h = harness();
  h.gateway.limit = undefined;
  await h.store.load();
  assert.equal(h.store.getState().limit, undefined);
  await h.store.stage("/temp/x", "x.txt");
  assert.deepEqual(h.gateway.created, []);
  assert.deepEqual(h.removed, ["/temp/x"]);
});

test("send uploads before it queues, then publishes and removes the draft", async () => {
  const h = harness();
  const draft = await staged(h);
  let uploadedBeforeQueue = false;
  h.conversation.onQueue = () => {
    uploadedBeforeQueue = h.gateway.uploads.length === 1;
    // The journal and the draft record the unknown outcome before the SDK call.
    assert.equal(h.journal.all()[0]?.phase, "queueing");
    assert.equal(h.drafts.all()[0]?.phase, "unknown");
  };
  await h.store.send(draft.id);
  assert.equal(uploadedBeforeQueue, true);
  assert.equal(h.conversation.queued.length, 1);
  assert.equal(h.conversation.published.length, 1);
  assert.deepEqual(h.drafts.all(), []);
  assert.deepEqual(h.store.getState().drafts, []);
  assert.deepEqual(h.journal.all(), []);
  assert.equal(h.env.changes, 1);
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

test("upload failures follow the SDK retry rule", async () => {
  const h = harness();
  const draft = await staged(h);
  h.gateway.uploadError = attachmentError(true);
  await h.store.send(draft.id);
  assert.deepEqual(
    [h.drafts.all()[0]?.phase, h.drafts.all()[0]?.retryable],
    ["failed", true],
  );
  // Retry uploads again.
  h.gateway.uploadError = attachmentError(false);
  await h.store.send(draft.id);
  assert.deepEqual(
    [h.drafts.all()[0]?.phase, h.drafts.all()[0]?.retryable],
    ["failed", false],
  );
  // A terminal failure cannot be sent again.
  await h.store.send(draft.id);
  assert.equal(h.gateway.uploads.length, 2);
});

test("discarding during the upload keeps the send from queuing", async () => {
  const h = harness();
  const draft = await staged(h);
  const gate = deferred();
  const upload = h.gateway.upload.bind(h.gateway);
  h.gateway.upload = async (file) => {
    await gate.promise;
    return upload(file);
  };
  const sending = h.store.send(draft.id);
  await h.store.discard(draft.id);
  gate.resolve();
  await sending;
  assert.equal(h.conversation.queued.length, 0);
  assert.equal(h.gateway.discarded.length, 1);
  assert.deepEqual(h.drafts.all(), []);
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

test("a reload while the upload runs keeps it uploading, and its failure shows", async () => {
  const h = harness();
  const draft = await staged(h);
  const gate = holdUploads(h);
  const sending = h.store.send(draft.id);
  // The chat regains focus, then a full load runs, during the upload.
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
});

test("a discard whose SDK delete fails still keeps the running send from queuing", async () => {
  const h = harness();
  const draft = await staged(h);
  const gate = holdUploads(h);
  h.gateway.discard = () => Promise.reject(attachmentError(true));
  const sending = h.store.send(draft.id);
  await h.store.discard(draft.id);
  gate.resolve();
  await sending;
  assert.equal(h.conversation.queued.length, 0);
  // The draft stays for another try.
  assert.equal(h.drafts.all()[0]?.id, draft.id);
});

test("a discarded file the SDK still lists is not imported again", async () => {
  const h = harness();
  const draft = await staged(h);
  // The SDK deleted its copy, but its pending list still names the file.
  h.gateway.discard = (file) => {
    h.gateway.discarded.push(file);
    return Promise.resolve();
  };
  await h.store.discard(draft.id);
  await h.store.load();
  assert.deepEqual(h.drafts.all(), []);
  assert.deepEqual(h.store.getState().drafts, []);
});

test("load imports SDK pending uploads as unassigned drafts and resets interrupted uploads", async () => {
  const preferences = new MemoryPreferences();
  const saved: FileDraft = {
    id: "S",
    conversationId: CONVERSATION,
    file: remoteFile("photo.jpg"),
    phase: "uploading",
    retryable: true,
  };
  new AttachmentDraftStore(preferences).save(saved);
  const h = harness({ preferences, conversationId: null });
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
  h.env.authorized = true;
  const gate = deferred();
  h.gateway.downloadGate = gate.promise;
  const first = h.store.download("m1", file);
  const second = h.store.download("m1", file);
  await second;
  gate.resolve();
  await first;
  assert.equal(h.gateway.downloads.length, 1);
  assert.equal(h.store.card("m1", file).phase, "verified");
  assert.equal(h.store.verified("m1")?.filename, "report.pdf");
});

test("a download failure shows Failed with the SDK retry rule", async () => {
  const h = harness();
  const file = remoteFile("report.pdf");
  h.gateway.downloadError = attachmentError(false);
  await h.store.download("m1", file);
  const card = h.store.card("m1", file);
  assert.deepEqual([card.phase, card.retryable], ["failed", false]);
});

test("a consent change drops verified files and a running download's result", async () => {
  const h = harness();
  const file = remoteFile("report.pdf");
  await h.store.download("m1", file);
  const gate = deferred();
  h.gateway.downloadGate = gate.promise;
  const running = h.store.download("m2", file);
  await Promise.resolve();
  await Promise.resolve();
  h.store.invalidateDownloads();
  gate.resolve();
  await running;
  assert.equal(h.store.card("m1", file).phase, "available");
  assert.equal(h.store.verified("m1"), undefined);
  assert.equal(h.store.card("m2", file).phase, "available");
  assert.equal(h.store.verified("m2"), undefined);
});

test("a closed conversation cannot download or send", async () => {
  const h = harness();
  const draft = await staged(h);
  h.env.allowed = false;
  await h.store.download("m1", remoteFile());
  await h.store.send(draft.id);
  assert.equal(h.gateway.downloads.length, 0);
  assert.equal(h.gateway.uploads.length, 0);
});

test("a second tap whose consent check ends after the first download does not download again", async () => {
  const h = harness();
  const file = remoteFile("report.pdf");
  const late = deferred<boolean>();
  let calls = 0;
  h.env.authorize = () => {
    calls += 1;
    return calls === 1 ? Promise.resolve(true) : late.promise;
  };
  const first = h.store.download("m1", file);
  const second = h.store.download("m1", file);
  await first;
  assert.equal(h.store.card("m1", file).phase, "verified");
  late.resolve(true);
  await second;
  assert.equal(h.gateway.downloads.length, 1);
});
