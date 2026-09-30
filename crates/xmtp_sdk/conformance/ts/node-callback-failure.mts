import assert from "node:assert/strict";
import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import * as sdk from "../../../../target/sdk-conformance/typescript-napi/index.ts";

/** Fail fast instead of waiting forever for a message that never comes. */
async function within<T>(promise: Promise<T>, what: string): Promise<T> {
  let timer: NodeJS.Timeout | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error(`${what} timed out`)), 20_000);
  });
  try {
    return await Promise.race([promise, timeout]);
  } finally {
    clearTimeout(timer);
  }
}

/**
 * on_value_failure_is_failed_and_unacked: when an `onValue` callback throws or
 * rejects, the stream closes once as failed with that error, and the item
 * stays unacknowledged, so the next default reader delivers it again.
 */
// verifies: PROC-028, PROC-041
export async function checkOnValueFailure(
  signer: sdk.Signer,
  backend: sdk.BackendOptions,
): Promise<void> {
  for (const mode of ["throw", "reject"] as const) {
    const client = await sdk.Client.create(signer, {
      backend,
      storage: {
        location: {
          path: join(await mkdtemp(join(tmpdir(), "f6-on-value-")), "client.db"),
        },
      },
      deviceSync: false,
    });
    const group = await client.conversations.createGroup([]);
    const ids = [
      await group.sendText("first"),
      await group.sendText("second"),
    ];
    // A third message makes an acknowledged second item fail fast instead of
    // waiting for a message that never comes.
    await group.sendText("third");
    const closed: sdk.StreamCloseReason[] = [];
    const stream = sdk.MessageStream.openGroup(client, group, undefined, {
      onClose: (reason) => closed.push(reason),
    });
    const failure = new Error(`callback ${mode}`);
    const seen: sdk.MessageId[] = [];
    await assert.rejects(
      within(stream.onValue((message) => {
        seen.push(message.id);
        if (seen.length < 2) return undefined;
        if (mode === "throw") throw failure;
        return Promise.reject(failure);
      }), `${mode}: onValue`),
      (error: unknown) => error === failure,
    );
    assert.deepEqual(seen, ids, `${mode}: the callback did not see both items`);
    assert.deepEqual(
      closed,
      [{ kind: "failed", error: failure }],
      `${mode}: the stream did not close once as failed`,
    );
    // The first item was acknowledged when the second was read; the second
    // was not, so the next default reader delivers it again.
    const reader = await group.messageReader();
    assert.equal(
      (await within(reader.next(), `${mode}: reader`))?.id,
      ids[1],
      `${mode}: the item was acknowledged`,
    );
    await reader.end();
    await client.end();
  }
}
