// The public TypeScript layer over the Node binding: objects, Client, and the
// host Message. Values must be public shapes, never binding classes.
import assert from "node:assert/strict";
import { realpathSync } from "node:fs";
import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import * as sdk from "../../../../target/sdk-conformance/typescript-napi/index.ts";
import {
  codecPolicyFailureNeverPublishes,
  customCodecPolicyAndIsolation,
} from "./codec-policy.mts";

const viemRoot = realpathSync(
  fileURLToPath(
    new URL("../../../../sdks/node/node_modules/viem", import.meta.url),
  ),
);
const { generatePrivateKey, privateKeyToAccount } = await import(
  pathToFileURL(join(viemRoot, "_esm/accounts/index.js")).href
);
const { toBytes } = await import(
  pathToFileURL(join(viemRoot, "_esm/index.js")).href
);

declare const gc: () => void;

function signerFor(): sdk.Signer {
  const account = privateKeyToAccount(generatePrivateKey());
  return {
    async identity() {
      return { kind: "ethereum", identifier: account.address.toLowerCase() };
    },
    async kind() {
      return { kind: "eoa" };
    },
    async sign(request) {
      const signature = await account.signMessage({ message: request.text });
      return { kind: "ecdsa", value: Uint8Array.from(toBytes(signature)) };
    },
  };
}

const backend: sdk.BackendOptions = { url: process.env.XMTP_BACKEND_URL! };

async function options(): Promise<sdk.ClientOptions> {
  const directory = await mkdtemp(join(tmpdir(), "xmtp-public-layer-"));
  return {
    backend,
    storage: { location: { path: join(directory, "client.db") } },
    deviceSync: false,
  };
}

// Fields whose public type is a string-literal union. A binding flat enum is a
// number, so a number here is an unconverted binding value.
const LITERAL_FIELDS = new Set([
  "kind",
  "deliveryStatus",
  "permissionLevel",
  "consentState",
  "action",
  "schema",
  "level",
  "category",
  "membershipState",
  "conversationType",
  "compression",
  "state",
]);
// Public objects are converted where they are created; do not look inside.
const PUBLIC_OBJECTS = [
  sdk.Client,
  sdk.Conversations,
  sdk.Group,
  sdk.Dm,
  sdk.Preferences,
  sdk.Diagnostics,
  sdk.Archives,
  sdk.Storage,
  sdk.Backend,
  sdk.SignatureRequest,
];
const PLAIN_PROTOTYPES = new Set<unknown>([
  Object.prototype,
  null,
  Array.prototype,
  Map.prototype,
  Set.prototype,
  sdk.Message.prototype,
]);

// A public value is plain data at every depth: no binding tag or payload, no
// binding class instance, no numeric enum, and no ArrayBuffer for bytes.
function assertPublic(value: unknown, path = "value", seen = new Set()): void {
  if (value === null || typeof value !== "object" || seen.has(value)) return;
  seen.add(value);
  assert.ok(!(value instanceof ArrayBuffer), `${path} is an ArrayBuffer`);
  if (value instanceof Uint8Array || value instanceof sdk.Timestamp) return;
  if (PUBLIC_OBJECTS.some((type) => value instanceof type)) return;
  assert.ok(
    PLAIN_PROTOTYPES.has(Object.getPrototypeOf(value)),
    `${path} is a ${value.constructor.name} instance, not plain data`,
  );
  assert.ok(!("tag" in value), `${path} has a binding tag`);
  assert.ok(!("inner" in value), `${path} has a binding payload`);
  const entries =
    value instanceof Map ? [...value.entries()] : Object.entries(value);
  for (const [key, item] of entries) {
    if (LITERAL_FIELDS.has(String(key)))
      assert.notEqual(typeof item, "number", `${path}.${key} is a number`);
    assertPublic(item, `${path}.${String(key)}`, seen);
  }
}

const alice = await sdk.Client.create(signerFor(), await options());
const bob = await sdk.Client.create(signerFor(), await options());

// The public Client keeps its binding private.
assert.equal("bindingClient" in alice, false);
assert.deepEqual(Object.keys(alice), []);

// One binding object lifts to one public object.
const conversations = alice.conversations;
assert.ok(conversations instanceof sdk.Conversations);
const group = await conversations.createGroup([]);
assert.ok(group instanceof sdk.Group);
assert.equal(group.kind, "group");
const fetched = await conversations.getById(group.id);
assert.ok(fetched instanceof sdk.Group, "getById returns the Group itself");
assert.equal(fetched.id, group.id);
const listed = await conversations.list();
assert.ok(listed.some((item) => item instanceof sdk.Group));

// Membership unions: inbox IDs and account identities take one parameter.
const bobIdentity = bob.identity;
assert.deepEqual(Object.keys(bobIdentity).sort(), ["identifier", "kind"]);
assert.equal(bobIdentity.kind, "ethereum");
const added = await group.addMembers([bobIdentity]);
assertPublic(added, "membership result");
assertPublic(await group.members(), "members");
assertPublic(bob.identity, "identity");
assert.ok((await group.members()).some((m) => m.inboxId === bob.inboxId));
// A mixed list fails before any call, so Bob stays a member.
await assert.rejects(group.removeMembers([bob.inboxId, bobIdentity] as never));
assert.ok((await group.members()).some((m) => m.inboxId === bob.inboxId));

// Messages are host Message objects with public-value fields.
const textId = await group.sendText("hello public layer");
const messages = await group.messages();
const text = messages.find((message) => message.id === textId);
assert.ok(text instanceof sdk.Message);
await customCodecPolicyAndIsolation(group, text, bob);
console.log("Node custom_codec_policy_and_isolation passed");
await codecPolicyFailureNeverPublishes(group, text);
console.log("Node codec_policy_failure_never_publishes passed");
assert.deepEqual(text.content, { kind: "text", value: "hello public layer" });
assert.equal(text.kind, "application");
assert.equal(text.deliveryStatus, "published");
assert.ok(text.sentAt instanceof sdk.Timestamp);
assert.ok(text.encoded.content instanceof Uint8Array);
assert.equal(typeof text.contentType.versionMajor, "number");
assert.ok(text.deliveryCursor?.startsWith("dc1_"));
for (const message of messages) assertPublic(message, `message ${message.id}`);
assert.equal(
  typeof (sdk.Message as unknown as { new?: unknown }).new,
  "undefined",
);

// Message actions go through the owner client and return public values.
assert.equal(text.client(), alice);
const replyId = await text.reply("a reply");
await text.react({ content: "👍", action: "added", schema: "unicode" });
const refreshed = await text.refresh();
assert.ok(refreshed instanceof sdk.Message);
assert.equal(refreshed.id, textId);
const reply = await conversations.getMessageById(replyId);
assert.ok(reply);
assert.equal(reply.content.kind, "reply");
assert.deepEqual(reply.replyContent, { kind: "text", value: "a reply" });
assert.equal((await reply.parent())?.id, textId);
assert.ok((await reply.conversation()) instanceof sdk.Group);
assertPublic(reply, "reply");
const reacted = await conversations.getMessageById(textId);
assert.equal(reacted?.reactions[0]?.reaction.action, "added");

// A DM and its optional identity results.
const dm = await conversations.createDm(bob.inboxId);
assert.ok(dm instanceof sdk.Dm);
assert.equal(await dm.peerInboxId(), bob.inboxId);
assert.equal(dm.creatorInboxId, alice.inboxId);
assert.ok((await conversations.getById(dm.id)) instanceof sdk.Dm);

// Client values and statics use public shapes.
assert.equal(alice.options.deviceSync, false);
assertPublic(alice.options, "options");
assertPublic(await alice.inboxState(false), "inbox state");
const reachable = await sdk.Client.canMessage([bobIdentity], backend);
assert.equal(reachable.get(`ethereum:${bobIdentity.identifier}`), true);
assertPublic(await sdk.Client.fetchServerConfiguration(backend), "config");

// The owner is weak: an ended client closes its messages' actions.
const aliceInboxId = alice.inboxId;
const groupId = group.id;
const groupTopic = group.topic;
await bob.end();
await alice.end();
// Getters read held values, so they stay readable after end (Decision 14).
// Calls fail with ClientClosed. The browser suite checks the same values.
assert.equal(alice.inboxId, aliceInboxId);
assert.ok(alice.conversations instanceof sdk.Conversations);
assert.equal(group.id, groupId);
assert.equal(group.topic, groupTopic);
await assert.rejects(
  group.sync(),
  (error) => error instanceof sdk.XmtpError.ClientClosed,
);
assert.throws(() => text.client(), /ClientClosed|client is closed/);
await assert.rejects(text.refresh(), /ClientClosed|client is closed/);

// A collected client also closes its messages' actions.
let collected: sdk.Client | undefined = await sdk.Client.create(
  signerFor(),
  await options(),
);
const orphanGroup = await collected.conversations.createGroup([]);
const orphanId = await orphanGroup.sendText("orphan");
const orphan = (await orphanGroup.messages()).find((m) => m.id === orphanId)!;
collected = undefined;
void orphanGroup;
for (let attempt = 0; attempt < 50; attempt += 1) {
  gc();
  await new Promise((resolve) => setTimeout(resolve, 20));
  try {
    orphan.client();
  } catch {
    break;
  }
}
assert.throws(() => orphan.client(), /ClientClosed|client is closed/);
console.log("Node public layer: objects, Client, and Message passed");
