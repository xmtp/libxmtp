// The browser private public entry in real Chromium, through the package
// worker: public objects over the worker proxies, `Group | Dm`, the host
// Message with public fields and actions, streams, events, public errors,
// worker-routed constructors and functions, and `Storage.admin()`.
// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { generatePrivateKey } from "../../../../sdks/browser/node_modules/viem/_esm/accounts/generatePrivateKey.js";
// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { privateKeyToAccount } from "../../../../sdks/browser/node_modules/viem/_esm/accounts/privateKeyToAccount.js";
// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { toBytes } from "../../../../sdks/browser/node_modules/viem/_esm/utils/encoding/toBytes.js";
import * as sdk from "../../../../target/sdk-generated/typescript-wasm/public-api.gen";

function check(condition: boolean, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

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
      const signed = await account.signMessage({ message: request.text });
      return { kind: "ecdsa", value: Uint8Array.from(toBytes(signed)) };
    },
  };
}

// A public value is plain data: no binding tag, payload, or class instance.
function isPublic(value: unknown, seen = new Set<unknown>()): boolean {
  if (value === null || typeof value !== "object" || seen.has(value))
    return true;
  seen.add(value);
  if (value instanceof ArrayBuffer) return false;
  if (value instanceof Uint8Array || value instanceof sdk.Timestamp)
    return true;
  const prototype: unknown = Object.getPrototypeOf(value);
  if (
    prototype !== Object.prototype &&
    prototype !== Array.prototype &&
    prototype !== Map.prototype &&
    prototype !== sdk.Message.prototype
  )
    return false;
  if ("tag" in value || "inner" in value) return false;
  const entries =
    value instanceof Map ? [...value.values()] : Object.values(value);
  return entries.every((item) => isPublic(item, seen));
}

const PROXY_MEMBERS = ["checkLive", "release", "handle", "session"];

export async function exercise(): Promise<string[]> {
  const results: string[] = [];
  const backend: sdk.BackendOptions = { url: `${location.origin}/backend` };
  const options: sdk.ClientOptions = {
    backend,
    storage: { location: "inMemory" },
    deviceSync: false,
  };
  const alice = await sdk.Client.create(signerFor(), options);
  const bob = await sdk.Client.create(signerFor(), options);
  try {
    // Public objects wrap the proxies and expose no transport member.
    const conversations = alice.conversations;
    check(conversations instanceof sdk.Conversations, "no Conversations");
    const group = await conversations.createGroup([bob.identity]);
    check(group instanceof sdk.Group, "createGroup did not return a Group");
    for (const name of PROXY_MEMBERS) {
      check(!(name in group), `Group exposes the proxy member ${name}`);
      check(!(name in alice), `Client exposes the proxy member ${name}`);
    }
    check(group.kind === "group", "Group kind is not a public literal");
    const fetched = await conversations.getById(group.id);
    check(fetched instanceof sdk.Group, "getById did not return the Group");
    const dm = await conversations.createDm(bob.inboxId);
    check(dm instanceof sdk.Dm, "createDm did not return a Dm");
    check((await dm.peerInboxId()) === bob.inboxId, "wrong DM peer");
    results.push("objects, Group | Dm, and no proxy members");

    // The host Message: public fields, actions, and the owner Client.
    const id = await group.sendText("public browser");
    const message = (await group.messages()).find((item) => item.id === id);
    check(message instanceof sdk.Message, "no public Message");
    check(message.content.kind === "text", "content is not a public union");
    check(message.deliveryStatus === "published", "status is not a literal");
    check(
      message.encoded.content instanceof Uint8Array,
      "bytes are not a view",
    );
    check(isPublic(message), "a Message field is not a public value");
    check(message.client() === alice, "Message did not return its Client");
    const replyId = await message.reply("reply");
    const reply = await conversations.getMessageById(replyId);
    check(reply?.content.kind === "reply", "reply is not a reply");
    check(reply.replyContent?.kind === "text", "reply body is not text");
    check((await reply.parent())?.id === id, "parent action failed");
    results.push("host Message fields and actions");

    // Streams and events yield public values.
    const stream = sdk.MessageStream.openGroup(alice, group);
    const streamedId = await group.sendText("streamed");
    let streamed: sdk.Message | undefined;
    for (;;) {
      const item = await stream.next();
      check(!item.done, "the stream ended early");
      if (item.value.id === streamedId) {
        streamed = item.value;
        break;
      }
    }
    await stream.end();
    check(streamed instanceof sdk.Message, "the stream yielded no Message");
    const joined = sdk.ConversationStream.open(alice);
    await joined.ready();
    const created = await conversations.createGroup([]);
    const next = await joined.next();
    check(next.value instanceof sdk.Group, "no Group from the stream");
    check(next.value.id === created.id, "wrong joined Group");
    await joined.end();
    const events = await alice.events({
      kinds: ["conversationJoined"],
      referencesOwnMessages: false,
    });
    await conversations.createGroup([]);
    const event = await events.next();
    check(event.value?.kind === "conversationJoined", "no public event");
    await events.return();
    results.push("streams and events");

    // Public errors and worker-routed constructors and functions.
    let invalid: unknown;
    try {
      await conversations.getMessageById("AB".repeat(32));
    } catch (error) {
      invalid = error;
    }
    check(
      invalid instanceof sdk.XmtpError.InvalidArgument &&
        invalid.details.category === "input" &&
        !("tag" in invalid),
      "a malformed ID did not fail with the public error",
    );
    const connected = await sdk.Backend.connect(backend);
    check(connected instanceof sdk.Backend, "Backend.connect failed");
    check(
      (await sdk.Client.inboxIdFor(alice.identity, connected)) ===
        alice.inboxId,
      "a static call with a connected Backend failed",
    );
    const configuration = await sdk.fetchServerConfiguration(backend);
    check(isPublic(configuration), "configuration is not a public value");
    results.push("public errors, Backend.connect, and worker functions");

    // Storage admin opens without a Client.
    const admin = await sdk.Storage.admin();
    check(typeof (await admin.poolCapacity()) === "number", "no capacity");
    await admin.end();
    results.push("Storage.admin");

    await alice.end();
    let closed: unknown;
    try {
      message.client();
    } catch (error) {
      closed = error;
    }
    check(
      closed instanceof sdk.XmtpError.ClientClosed,
      "an ended Client's Message did not fail with ClientClosed",
    );
    results.push("ClientClosed after end");
  } finally {
    await alice.end().catch(() => undefined);
    await bob.end();
  }
  return results;
}
