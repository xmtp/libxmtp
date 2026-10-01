import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { setImmediate as nextTurn } from "node:timers/promises";

import {
  ReaderStream,
  type ReaderLike,
  type StreamCloseReason,
} from "../../../../target/sdk-conformance/typescript-napi/runtime/streams/reader.ts";
import * as B from "../../../../target/sdk-conformance/typescript-napi/xmtp_sdk.ts";
import {
  dispose,
  drained,
  lifetimeCycles,
  rawOptions,
  signal,
  within,
} from "./callback-lifetime-support.mts";

const families = [
  "messageValue",
  "conversationValue",
  "messageConnection",
  "conversationConnection",
  "messageClose",
  "conversationClose",
] as const;
type Family = (typeof families)[number];
type Message = Exclude<
  Awaited<ReturnType<B.MessageReaderLike["next"]>>,
  undefined
>;
type Value = Message | B.Conversation;
type Reader = B.MessageReaderLike | B.ConversationReaderLike;

function disposeConversation(value: B.Conversation) {
  if (B.Conversation.Group.instanceOf(value)) dispose(value.inner.group);
  else dispose(value.inner.dm);
}

async function streamLifetimeCycle(
  backendURL: string,
  family: Family,
  endClient: boolean,
) {
  const directory = await mkdtemp(join(tmpdir(), "xmtp-stream-callback-"));
  const path = join(directory, "client.db3");
  const options = rawOptions(backendURL, {
    location: B.StorageLocation.Explicit.new({
      dbPath: path,
      attachmentsDir: `${path}.attachments`,
    }),
    singleConnection: true,
  });
  const signer = await B.generateLocalSigner();
  const client = await B.Client.create(signer, options);
  const conversations = client.conversations();
  const group = await conversations.createGroup([], undefined);
  const groupId = group.id();
  let reopened: B.ClientLike | undefined;
  let reopenedConversations: B.ConversationsLike | undefined;
  let reopenedGroup: B.GroupLike | undefined;
  const readers: Reader[] = [];
  const extraGroups: B.GroupLike[] = [];
  const receivedConversations: B.Conversation[] = [];
  const entered = signal();
  const release = signal();
  const returned = signal();
  const message = family.startsWith("message");
  const valueCallback = family.endsWith("Value");
  const connectionCallback = family.endsWith("Connection");
  let calls = 0;
  let active = 0;
  let closes = 0;
  let closeReason: StreamCloseReason | undefined;
  let reentry: Promise<void> | undefined;
  let consuming: Promise<void> | undefined;
  let firstId: B.MessageId | undefined;
  let secondId: B.MessageId | undefined;

  const open = async (abort: AbortSignal): Promise<ReaderLike<Value>> => {
    const reader = message
      ? await group.messageReader(undefined, { signal: abort })
      : await conversations.conversationReader(undefined, { signal: abort });
    readers.push(reader);
    return reader;
  };
  const reenter = () => {
    // Connection and close callbacks have synchronous return types. Start
    // the async reentry here and observe its completion outside that call.
    calls++;
    if (reentry) return;
    entered.resolve();
    reentry = endClient ? client.end() : stream.end();
  };
  const stream = new ReaderStream<Value>(open, client, {
    onClose(reason) {
      closes++;
      closeReason = reason;
      if (!valueCallback && !connectionCallback) reenter();
    },
    onConnectionStateChange: connectionCallback ? reenter : undefined,
  });
  try {
    await within(stream.ready(), `${family} ready`);
    if (valueCallback) {
      if (message) {
        firstId = await group.sendText("first held value", undefined);
        secondId = await group.sendText("queued value", undefined);
      } else {
        extraGroups.push(await conversations.createGroupOptimistic(undefined));
        extraGroups.push(await conversations.createGroupOptimistic(undefined));
      }
      consuming = stream.onValue(async (value) => {
        calls++;
        active++;
        assert.equal(calls, 1, "a later value reached the callback");
        if (message) assert.equal((value as Message).id, firstId);
        else receivedConversations.push(value as B.Conversation);
        entered.resolve();
        try {
          await release.promise;
          if (endClient) await client.end();
          else await stream.end();
        } finally {
          active--;
          returned.resolve();
        }
      });
      await within(entered.promise, `${family} callback entry`);
      await nextTurn();
      assert.equal(active, 1);
      assert.equal(calls, 1, "the held callback did not stop the next handoff");
      release.resolve();
      await within(returned.promise, `${family} callback return`);
      await within(consuming, `${family} stream end from callback`).catch(
        (error: unknown) => {
          if (!endClient || !(error instanceof B.XmtpError.ClientClosed))
            throw error;
        },
      );
      assert.equal(active, 0);
      assert.equal(calls, 1);
      if (endClient) {
        await assert.rejects(
          within(stream.next(), "stream next after client end"),
          B.XmtpError.ClientClosed,
        );
      } else
        assert.equal(
          (await within(stream.next(), "stream next after end")).done,
          true,
        );
      if (message) {
        // Ending inside onValue must not acknowledge that value. A new
        // request acknowledges it only when the replacement asks for more.
        let replayGroup = group;
        if (endClient) {
          reopened = await B.Client.build(
            await signer.identity(),
            options,
            undefined,
          );
          reopenedConversations = reopened.conversations();
          const found = await reopenedConversations.getById(groupId);
          assert.ok(found && B.Conversation.Group.instanceOf(found));
          reopenedGroup = found.inner.group;
          replayGroup = reopenedGroup;
        }
        const replay = await replayGroup.messageReader(undefined);
        readers.push(replay);
        assert.equal(
          (await within(replay.next(), "unacknowledged replay"))?.id,
          firstId,
        );
        assert.equal(
          (await within(replay.next(), "next-request acknowledgement"))?.id,
          secondId,
        );
        await replay.end();
        const afterAck = await replayGroup.messageReader(undefined);
        readers.push(afterAck);
        assert.equal(
          (await within(afterAck.next(), "durable acknowledgement"))?.id,
          secondId,
        );
        await afterAck.end();
      }
    } else {
      if (!connectionCallback) await stream.end();
      await within(entered.promise, `${family} callback entry`);
      assert.ok(reentry);
      await within(reentry, `${family} end reentry`);
      if (endClient && connectionCallback) {
        await assert.rejects(
          within(stream.next(), "stream next after client end"),
          B.XmtpError.ClientClosed,
        );
      } else
        assert.equal(
          (await within(stream.next(), "stream next after end")).done,
          true,
        );
      await nextTurn();
      assert.equal(calls, 1, "a callback restarted after end");
    }
    if (endClient)
      await assert.rejects(
        within(conversations.list(undefined), "client closed after callback"),
        B.XmtpError.ClientClosed,
      );
    assert.equal(closes, 1, "stream close was not delivered exactly once");
    assert.ok(closeReason);
    if (endClient && (valueCallback || connectionCallback)) {
      assert.equal(closeReason.kind, "failed");
      assert.ok(
        closeReason.kind === "failed" &&
          closeReason.error instanceof B.XmtpError.ClientClosed,
      );
    } else assert.equal(closeReason.kind, "closed");
  } finally {
    release.resolve();
    await stream.end();
    if (consuming) await consuming.catch(() => {});
    if (reentry) await reentry.catch(() => {});
    for (const reader of readers) {
      await reader.end().catch((error: unknown) => {
        if (!(error instanceof B.XmtpError.ClientClosed)) throw error;
      });
      dispose(reader);
    }
    await client.end();
    await reopened?.end();
    if (reopenedGroup) dispose(reopenedGroup);
    if (reopenedConversations) dispose(reopenedConversations);
    if (reopened) dispose(reopened);
    for (const conversation of receivedConversations)
      disposeConversation(conversation);
    for (const created of extraGroups) dispose(created);
    dispose(group);
    dispose(conversations);
    dispose(client);
    dispose(signer);
    await rm(directory, { recursive: true, force: true });
  }
  await drained();
}

export async function checkNodeStreamLifetime(
  backendURL: string,
  selected?: string,
) {
  for (const family of families) {
    for (const endClient of [false, true]) {
      const name = endClient ? `${family}ClientEnd` : family;
      if (selected && selected !== name) continue;
      for (let cycle = 0; cycle < lifetimeCycles; cycle++)
        await streamLifetimeCycle(backendURL, family, endClient);
      console.log(
        `Node stream lifetime: ${name}, ${lifetimeCycles} cycles passed`,
      );
    }
  }
}
