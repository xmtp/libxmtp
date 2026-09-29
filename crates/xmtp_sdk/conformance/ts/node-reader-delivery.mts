import assert from "node:assert/strict";

import * as sdk from "../../../../target/sdk-conformance/typescript-napi/public-api.gen.ts";
// Fake-reader cases drive the shared host reader stream directly.
import {
  MessageStream as HostMessageStream,
  type StreamCloseReason as HostCloseReason,
} from "../../../../target/sdk-conformance/typescript-napi/runtime/streams/reader.ts";

export async function readerDelivery(reopened: sdk.Client) {
  const reopenedGroup = await reopened.conversations.createGroup([]);
  const reader = await reopenedGroup.messageReader();
  const messageId = await reopenedGroup.sendText("durable stream");
  const first = await reader.next();
  assert.equal(first?.id.toString(), messageId.toString());
  await reader.end();
  const replay = await reopenedGroup.messageReader();
  const repeated = await replay.next();
  assert.equal(repeated?.id.toString(), messageId.toString());
  await replay.end();
  const stream = sdk.MessageStream.openGroup(reopened, reopenedGroup);
  assert.equal(
    (await stream.next()).value?.id.toString(),
    messageId.toString(),
  );
  const pending = stream.next();
  setTimeout(() => void stream.return(), 50);
  assert.equal((await pending).done, true);
  await stream.return();
  const protocolGroup = await reopened.conversations.createGroup([]);
  const firstId = await protocolGroup.sendText("ack on request");
  const firstStream = sdk.MessageStream.openGroup(reopened, protocolGroup);
  assert.equal(
    (await firstStream.next()).value?.id.toString(),
    firstId.toString(),
  );
  await firstStream.return();
  const secondStream = sdk.MessageStream.openGroup(reopened, protocolGroup);
  let replayTimer: ReturnType<typeof setTimeout>;
  const replayedItem = await Promise.race([
    secondStream.next(),
    new Promise<never>((_, reject) => {
      replayTimer = setTimeout(
        () => reject(new Error("adapter prefetched and acknowledged the item")),
        3_000,
      );
    }),
  ]).finally(() => clearTimeout(replayTimer));
  assert.equal(
    replayedItem.value?.id.toString(),
    firstId.toString(),
    "item was prefetched and acknowledged",
  );
  const secondId = await protocolGroup.sendText("second request");
  assert.equal(
    (await secondStream.next()).value?.id.toString(),
    secondId.toString(),
  );
  await secondStream.return();
  const afterAck = await protocolGroup.messageReader();
  assert.equal(
    (await afterAck.next())?.id.toString(),
    secondId.toString(),
    "first item was not acknowledged on next request",
  );
  await afterAck.end();
  const breakGroup = await reopened.conversations.createGroup([]);
  const breakId = await breakGroup.sendText("close after break");
  const breakReasons: sdk.StreamCloseReason[] = [];
  const retainedStream = sdk.MessageStream.openGroup(
    reopened,
    breakGroup,
    undefined,
    { onClose: (reason) => breakReasons.push(reason) },
  );
  for await (const value of retainedStream) {
    assert.equal(value.id.toString(), breakId.toString());
    break;
  }
  assert.deepEqual(
    breakReasons.map((reason) => reason.kind),
    ["closed"],
    "break did not close the stored stream",
  );
  const breakReplay = await breakGroup.messageReader();
  assert.equal(
    (await breakReplay.next())?.id.toString(),
    breakId.toString(),
    "break acknowledged the last message",
  );
  await breakReplay.end();

  let resolveCreation!: (reader: {
    next: () => Promise<undefined>;
    end: () => Promise<void>;
  }) => void;
  let markCreationStarted!: () => void;
  const creationStarted = new Promise<void>((resolve) => {
    markCreationStarted = resolve;
  });
  let endedLate = false;
  const opening = new HostMessageStream<undefined>(
    () =>
      new Promise((resolve) => {
        resolveCreation = resolve;
        markCreationStarted();
      }),
    reopened,
  );
  const openingRead = opening.next();
  await creationStarted;
  const openingEnd = opening.end();
  resolveCreation({
    next: async () => undefined,
    end: async () => {
      endedLate = true;
    },
  });
  await openingEnd;
  assert.equal((await openingRead).done, true, "opening read did not settle");
  assert.equal(endedLate, true, "late reader remained open");
  let pendingScopeOwned = false;
  let pendingOpenStarted!: () => void;
  const pendingOpenStartedSignal = new Promise<void>((resolve) => {
    pendingOpenStarted = resolve;
  });
  let releasePendingOpen!: (reader: {
    next: () => Promise<undefined>;
    end: () => Promise<void>;
  }) => void;
  let pendingReplacement: HostMessageStream<unknown> | undefined;
  const pendingScopeOpen = async () => {
    if (pendingScopeOwned)
      throw Object.assign(new Error("stream scope is still owned"), {
        code: "ConsumerOwned",
      });
    pendingScopeOwned = true;
    return {
      next: async () => undefined,
      end: async () => {
        pendingScopeOwned = false;
      },
    };
  };
  const pendingScopeStream = new HostMessageStream(
    async () => {
      pendingScopeOwned = true;
      pendingOpenStarted();
      return new Promise<Awaited<ReturnType<typeof pendingScopeOpen>>>(
        (resolve) => {
          releasePendingOpen = resolve;
        },
      );
    },
    reopened,
    {
      onClose: (reason) => {
        assert.equal(reason.kind, "closed");
        pendingReplacement = new HostMessageStream(pendingScopeOpen, reopened);
      },
    },
  );
  const pendingScopeRead = pendingScopeStream.next();
  await pendingOpenStartedSignal;
  const pendingScopeEnd = pendingScopeStream.end();
  releasePendingOpen({
    next: async () => undefined,
    end: async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
      pendingScopeOwned = false;
    },
  });
  await pendingScopeEnd;
  assert.equal((await pendingScopeRead).done, true);
  assert.ok(pendingReplacement, "pending open did not call onClose");
  await pendingReplacement.ready();
  await pendingReplacement.end();
  let rejectPendingOpen!: (error: Error) => void;
  let failedOpenStarted!: () => void;
  const failedOpenStartedSignal = new Promise<void>((resolve) => {
    failedOpenStarted = resolve;
  });
  const failedOpenReasons: HostCloseReason[] = [];
  const failedPendingStream = new HostMessageStream(
    () => {
      failedOpenStarted();
      return new Promise<never>((_, reject) => {
        rejectPendingOpen = reject;
      });
    },
    reopened,
    { onClose: (reason) => failedOpenReasons.push(reason) },
  );
  await failedOpenStartedSignal;
  const failedPendingEnd = failedPendingStream.end();
  rejectPendingOpen(new Error("open failed after end"));
  await failedPendingEnd;
  assert.deepEqual(
    failedOpenReasons.map((reason) => reason.kind),
    ["closed"],
  );
  assert.equal((await failedPendingStream.next()).done, true);
  return { first, messageId };
}
