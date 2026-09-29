import assert from "node:assert/strict";

import * as sdk from "../../../../target/sdk-conformance/typescript-napi/index.ts";
import type { readerDelivery } from "./node-reader-delivery.mts";
import { assertNoUnhandledRejection } from "./node-support.mts";

export async function streamFailures(
  reopened: sdk.Client,
  { first, messageId }: Awaited<ReturnType<typeof readerDelivery>>,
): Promise<void> {
  const callbackGroup = await reopened
    .conversations()
    .createGroup([], undefined);
  const callbackId = await callbackGroup.sendText("callback acknowledgment");
  let releaseCallback!: () => void;
  const callbackGate = new Promise<void>((resolve) => {
    releaseCallback = resolve;
  });
  let callbackEntered!: () => void;
  const entered = new Promise<void>((resolve) => {
    callbackEntered = resolve;
  });
  const callbackStream = new sdk.MessageStream(
    (signal) => callbackGroup.messageReader(undefined, { signal }),
    reopened,
  );
  const consumption = callbackStream.onValue(async (value) => {
    assert.equal(value.id.toString(), callbackId.toString());
    callbackEntered();
    await callbackGate;
  });
  await entered;
  await callbackStream.end();
  const callbackReplay = await callbackGroup.messageReader();
  assert.equal(
    (await callbackReplay.next())?.id.toString(),
    callbackId.toString(),
  );
  await callbackReplay.end();
  releaseCallback();
  await consumption;
  const conversationStream = new sdk.ConversationStream(
    (signal) =>
      reopened.conversations().conversationReader(undefined, { signal }),
    reopened,
  );
  await conversationStream.ready();
  await reopened.conversations().createGroup([], undefined);
  assert.equal((await conversationStream.next()).done, false);
  await conversationStream.end();
  // verifies: CONS-030
  const consentReader = sdk.ConversationStream.open(reopened, {
    consentStates: [sdk.ConsentState.Allowed],
  });
  const deniedConversation = await reopened
    .conversations()
    .createGroup([], undefined);
  await reopened.preferences().setConsentStates([
    {
      entity: new sdk.ConsentEntity.Conversation({
        conversationId: deniedConversation.id(),
      }),
      state: sdk.ConsentState.Denied,
    },
  ]);
  const allowedConversation = await reopened
    .conversations()
    .createGroup([], undefined);
  const selectedConversation = (
    await Promise.race([
      consentReader.next(),
      new Promise<never>((_, reject) =>
        setTimeout(
          () => reject(new Error("selected conversation not delivered")),
          5_000,
        ),
      ),
    ])
  ).value;
  assert.equal(selectedConversation?.tag, sdk.Conversation_Tags.Group);
  assert.equal(
    (
      selectedConversation as InstanceType<typeof sdk.Conversation.Group>
    ).inner.group
      .id()
      .toString(),
    allowedConversation.id().toString(),
  );
  await consentReader.end();
  let markAbortReady!: () => void;
  const abortReady = new Promise<void>((resolve) => {
    markAbortReady = resolve;
  });
  const rejectedOpening = new sdk.MessageStream(
    (signal) =>
      new Promise((_, reject) => {
        signal.addEventListener("abort", () =>
          reject(new DOMException("aborted", "AbortError")),
        );
        markAbortReady();
      }),
    reopened,
  );
  const rejectedRead = rejectedOpening.next();
  await abortReady;
  await rejectedOpening.return();
  assert.equal((await rejectedRead).done, true);
  const creationFailure = Object.assign(new Error("reader creation failed"), {
    code: "Storage",
  });
  const creationReasons: sdk.StreamCloseReason[] = [];
  const failedOpening = new sdk.MessageStream(
    async () => {
      throw creationFailure;
    },
    reopened,
    { onClose: (reason) => creationReasons.push(reason) },
  );
  await assert.rejects(
    failedOpening.next(),
    (error) => error === creationFailure,
  );
  assert.equal(creationReasons[0]?.kind, "failed");

  const openFailureWithCloseThrow = new Error("reader open failed");
  let failedOpenCloseCalls = 0;
  await assertNoUnhandledRejection(async () => {
    const stream = new sdk.MessageStream(
      async () => {
        throw openFailureWithCloseThrow;
      },
      reopened,
      {
        onClose: () => {
          failedOpenCloseCalls += 1;
          throw new Error("open failure close callback failed");
        },
      },
    );
    await assert.rejects(
      stream.next(),
      (error) => error === openFailureWithCloseThrow,
    );
  });
  assert.equal(failedOpenCloseCalls, 1);

  let readerLeaseHeld = false;
  const readFailure = new Error("injected reader failure");
  const failedStream = new sdk.MessageStream(async () => {
    assert.equal(readerLeaseHeld, false);
    readerLeaseHeld = true;
    return {
      next: async () => {
        throw readFailure;
      },
      end: async () => {
        await new Promise((resolve) => setTimeout(resolve, 0));
        readerLeaseHeld = false;
      },
    };
  }, reopened);
  await assert.rejects(
    async () => {
      for await (const message of failedStream) {
        assert.fail(`unexpected message: ${message.id}`);
      }
    },
    (error) => error === readFailure,
  );
  const replacementStream = new sdk.MessageStream(async () => {
    assert.equal(readerLeaseHeld, false, "failed stream kept the reader lease");
    readerLeaseHeld = true;
    return {
      next: async () => first!,
      end: async () => {
        readerLeaseHeld = false;
      },
    };
  }, reopened);
  assert.equal(
    (await replacementStream.next()).value?.id.toString(),
    messageId.toString(),
  );
  await replacementStream.return();
  const endFailure = new Error("injected reader end failure");
  const failedEndStream = new sdk.MessageStream(
    async () => ({
      next: async () => {
        throw readFailure;
      },
      end: async () => {
        throw endFailure;
      },
    }),
    reopened,
  );
  await assert.rejects(
    failedEndStream.next(),
    (error) => error === readFailure,
  );
}
