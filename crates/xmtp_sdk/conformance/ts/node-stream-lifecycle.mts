import assert from "node:assert/strict";

import * as sdk from "../../../../target/sdk-conformance/typescript-napi/index.ts";
import { assertNoUnhandledRejection } from "./node-support.mts";

export async function streamLifecycle(reopened: sdk.Client): Promise<void> {
  // verifies: PROC-041, PROC-042
  for (const StreamType of [sdk.MessageStream, sdk.ConversationStream]) {
    const closeReasons: sdk.StreamCloseReason[] = [];
    const explicitlyClosed = new StreamType(
      async () => ({ next: async () => undefined, end: async () => {} }),
      reopened,
      { onClose: (reason) => closeReasons.push(reason) },
    );
    await explicitlyClosed.end();
    await explicitlyClosed.end();
    assert.deepEqual(
      closeReasons.map((reason) => reason.kind),
      ["closed"],
    );
  }
  for (const closeMode of ["end", "fail"] as const) {
    let scopeOwned = false;
    const openScope = async () => {
      if (scopeOwned)
        throw Object.assign(new Error("stream scope is still owned"), {
          code: "ConsumerOwned",
        });
      scopeOwned = true;
      return {
        next: async () => undefined,
        end: async () => {
          await new Promise((resolve) => setTimeout(resolve, 0));
          scopeOwned = false;
        },
      };
    };
    const readFailure = new Error("reader failed before close");
    let replacement: sdk.MessageStream | undefined;
    const stream = new sdk.MessageStream(
      async () => ({
        ...(await openScope()),
        next: async () => {
          if (closeMode === "fail") throw readFailure;
          return undefined;
        },
      }),
      reopened,
      {
        onClose: (reason) => {
          assert.equal(reason.kind, closeMode === "end" ? "closed" : "failed");
          replacement = new sdk.MessageStream(openScope, reopened);
        },
      },
    );
    await stream.ready();
    if (closeMode === "end") await stream.end();
    else await assert.rejects(stream.next(), (error) => error === readFailure);
    assert.ok(replacement, "close callback did not reopen the stream scope");
    await replacement.ready();
    await replacement.end();
  }
  // A second close waits for the reader teardown that the first close started.
  for (const firstClose of ["end", "fail"] as const) {
    let readerEnds = 0;
    let readerEnded = false;
    let releaseReaderEnd!: () => void;
    let markReaderEndStarted!: () => void;
    const readerEndStarted = new Promise<void>((resolve) => {
      markReaderEndStarted = resolve;
    });
    const readFailure = new Error("reader failed during close");
    const racing = new sdk.MessageStream(
      async () => ({
        next: async () => {
          throw readFailure;
        },
        end: async () => {
          readerEnds++;
          markReaderEndStarted();
          await new Promise<void>((resolve) => {
            releaseReaderEnd = resolve;
          });
          readerEnded = true;
        },
      }),
      reopened,
    );
    await racing.ready();
    const first =
      firstClose === "end"
        ? racing.end()
        : assert.rejects(racing.next(), (error) => error === readFailure);
    await readerEndStarted;
    let secondSettled = false;
    const second = racing.end().then(() => {
      secondSettled = true;
    });
    await new Promise((resolve) => setImmediate(resolve));
    assert.equal(
      secondSettled,
      false,
      `end() returned before the first ${firstClose} ended the reader`,
    );
    releaseReaderEnd();
    await Promise.all([first, second]);
    assert.equal(readerEnded, true);
    assert.equal(readerEnds, 1, "concurrent closes ended the reader twice");
  }
  let endedAfterCloseThrow = false;
  const throwingClose = new sdk.MessageStream(
    async () => ({
      next: async () => undefined,
      end: async () => {
        endedAfterCloseThrow = true;
      },
    }),
    reopened,
    {
      onClose: () => {
        throw new Error("close callback failed");
      },
    },
  );
  await throwingClose.ready();
  await throwingClose.end();
  assert.equal(
    endedAfterCloseThrow,
    true,
    "throwing onClose skipped reader.end",
  );
  assert.equal((await throwingClose.next()).done, true);
  const throwingEndOfStream = new sdk.MessageStream(
    async () => ({ next: async () => undefined, end: async () => {} }),
    reopened,
    {
      onClose: () => {
        throw new Error("end of stream callback failed");
      },
    },
  );
  await throwingEndOfStream.ready();
  assert.equal((await throwingEndOfStream.next()).done, true);
  const endedSignal = new AbortController();
  const endedWithSignal = new sdk.MessageStream(
    async () => ({ next: async () => undefined, end: async () => {} }),
    reopened,
    { signal: endedSignal.signal },
  );
  await endedWithSignal.ready();
  let returnCallsAfterEnd = 0;
  const originalReturn = endedWithSignal.return.bind(endedWithSignal);
  endedWithSignal.return = async () => {
    returnCallsAfterEnd++;
    return originalReturn();
  };
  await endedWithSignal.end();
  endedSignal.abort();
  assert.equal(
    returnCallsAfterEnd,
    0,
    "abort handler remained after stream end",
  );
  for (const abortBeforeOpen of [true, false]) {
    const controller = new AbortController();
    if (abortBeforeOpen) controller.abort();
    let endedAfterAbort = false;
    let opened = false;
    await assertNoUnhandledRejection(async () => {
      const aborted = new sdk.MessageStream(
        async () => {
          opened = true;
          return {
            next: async () => undefined,
            end: async () => {
              endedAfterAbort = true;
            },
          };
        },
        reopened,
        {
          signal: controller.signal,
          onClose: () => {
            throw new Error("abort close callback failed");
          },
        },
      );
      if (!abortBeforeOpen) {
        await aborted.ready();
        controller.abort();
      }
      await new Promise((resolve) => setImmediate(resolve));
      // A stream aborted before its opener starts never opens; one aborted
      // after opening ends its reader. Either way no reader stays open.
      assert.equal(
        !opened || endedAfterAbort,
        true,
        "aborted reader remained open",
      );
      if (!abortBeforeOpen) assert.equal(opened, true, "opener did not run");
      assert.equal((await aborted.next()).done, true);
    });
  }
  let endedAfterFailureCloseThrow = false;
  const throwingFailureClose = new sdk.MessageStream(
    async () => ({
      next: async () => {
        throw new Error("reader failed");
      },
      end: async () => {
        endedAfterFailureCloseThrow = true;
      },
    }),
    reopened,
    {
      onClose: () => {
        throw new Error("failure callback failed");
      },
    },
  );
  await throwingFailureClose.ready();
  await assert.rejects(throwingFailureClose.next(), /reader failed/);
  assert.equal(
    endedAfterFailureCloseThrow,
    true,
    "throwing failure callback skipped reader.end",
  );
  let stateCallbackCalls = 0;
  const throwingState = new sdk.MessageStream(
    async () => ({
      next: async () => undefined,
      end: async () => {},
      connectionState: async () => sdk.ConnectionState.Connected,
      connectionStateChanged: async () => sdk.ConnectionState.Closed,
    }),
    reopened,
    {
      onConnectionStateChange: () => {
        stateCallbackCalls += 1;
        throw new Error("state callback failed");
      },
    },
  );
  await throwingState.ready();
  await new Promise((resolve) => setTimeout(resolve, 10));
  assert.equal(stateCallbackCalls, 1);
  await throwingState.end();
  // verifies: PROC-041
  for (const code of [
    "RecoveryExhausted",
    "Storage",
    "Lagged",
    "CredentialRejected",
    "CredentialExhausted",
    "BackendMismatch",
    "ClientVersionTooOld",
    "ConsumerOwned",
    "ForeignCursor",
  ]) {
    const failure = Object.assign(new Error(code), { code });
    const reasons: sdk.StreamCloseReason[] = [];
    const failing = new sdk.MessageStream(
      async () => ({
        next: async () => {
          throw failure;
        },
        end: async () => {},
      }),
      reopened,
      { onClose: (reason) => reasons.push(reason) },
    );
    await assert.rejects(failing.next(), (error) => error === failure);
    assert.equal(reasons.length, 1);
    assert.equal(reasons[0].kind, "failed");
    if (reasons[0].kind === "failed")
      assert.equal((reasons[0].error as { code: string }).code, code);
  }
  // verifies: PROC-044
  for (const StreamType of [sdk.MessageStream, sdk.ConversationStream]) {
    const states: sdk.ConnectionState[] = [];
    const changes: Array<(state: sdk.ConnectionState) => void> = [];
    const probe = new StreamType(
      async () => ({
        next: async () => undefined,
        end: async () => {},
        connectionState: async () => sdk.ConnectionState.Connected,
        connectionStateChanged: () =>
          new Promise<sdk.ConnectionState>((resolve) => changes.push(resolve)),
      }),
      reopened,
      { onConnectionStateChange: (_previous, current) => states.push(current) },
    );
    await probe.ready();
    // A reader opened on a connected connection reports Connected first.
    assert.deepEqual(states, [sdk.ConnectionState.Connected]);
    changes.shift()?.(sdk.ConnectionState.Reconnecting);
    await new Promise((resolve) => setTimeout(resolve, 0));
    changes.shift()?.(sdk.ConnectionState.Connected);
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert.deepEqual(states, [
      sdk.ConnectionState.Connected,
      sdk.ConnectionState.Reconnecting,
      sdk.ConnectionState.Connected,
    ]);
    await probe.end();
  }
  let closedStatePolls = 0;
  const closedStateProbe = new sdk.MessageStream(
    async () => ({
      next: async () => undefined,
      end: async () => {},
      connectionState: async () => sdk.ConnectionState.Closed,
      connectionStateChanged: async () => {
        closedStatePolls += 1;
        if (closedStatePolls > 2) throw new Error("closed state loop");
        return sdk.ConnectionState.Closed;
      },
    }),
    reopened,
    { onConnectionStateChange: () => {} },
  );
  await closedStateProbe.ready();
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(closedStatePolls, 0, "closed state kept the monitor running");
  await closedStateProbe.end();
}
