import assert from "node:assert/strict";

import * as sdk from "../../../../target/sdk-conformance/typescript-napi/index.ts";
// Fake-reader cases drive the shared host reader stream directly.
import {
  MessageStream as HostMessageStream,
  type StreamCloseReason as HostCloseReason,
} from "../../../../target/sdk-conformance/typescript-napi/runtime/streams/reader.ts";
import type { readerDelivery } from "./node-reader-delivery.mts";
import { assertNoUnhandledRejection } from "./node-support.mts";

export async function streamFailures(
  reopened: sdk.Client,
  { first, messageId }: Awaited<ReturnType<typeof readerDelivery>>,
): Promise<void> {
  const callbackGroup = await reopened.conversations.createGroup([]);
  const callbackId = await callbackGroup.sendText("callback acknowledgment");
  let releaseCallback!: () => void;
  const callbackGate = new Promise<void>((resolve) => {
    releaseCallback = resolve;
  });
  let callbackEntered!: () => void;
  const entered = new Promise<void>((resolve) => {
    callbackEntered = resolve;
  });
  const callbackStream = sdk.MessageStream.openGroup(reopened, callbackGroup);
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
  const conversationStream = sdk.ConversationStream.open(reopened);
  await conversationStream.ready();
  await reopened.conversations.createGroup([]);
  assert.equal((await conversationStream.next()).done, false);
  await conversationStream.end();
  // verifies: CONS-030
  const consentReader = sdk.ConversationStream.open(reopened, {
    consentStates: ["allowed"],
  });
  const deniedConversation = await reopened.conversations.createGroup([]);
  await reopened.preferences.setConsentStates([
    {
      entity: { kind: "conversation", conversationId: deniedConversation.id },
      state: "denied",
    },
  ]);
  const allowedConversation = await reopened.conversations.createGroup([]);
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
  assert.ok(selectedConversation instanceof sdk.Group);
  assert.equal(selectedConversation.id, allowedConversation.id);
  await consentReader.end();
  let markAbortReady!: () => void;
  const abortReady = new Promise<void>((resolve) => {
    markAbortReady = resolve;
  });
  const rejectedOpening = new HostMessageStream(
    (signal) =>
      new Promise<never>((_, reject) => {
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
  const creationReasons: HostCloseReason[] = [];
  const failedOpening = new HostMessageStream(
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
    const stream = new HostMessageStream(
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
  const failedStream = new HostMessageStream(async () => {
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
        assert.fail(`unexpected message: ${String(message)}`);
      }
    },
    (error) => error === readFailure,
  );
  const replacementStream = new HostMessageStream(async () => {
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
  const failedEndStream = new HostMessageStream(
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
