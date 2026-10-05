import assert from "node:assert/strict";

import * as sdk from "../../../../target/sdk-conformance/typescript-napi/index.ts";

type Scope = "group" | "all";

async function bounded<T>(value: Promise<T>, step: string): Promise<T> {
  let timer: ReturnType<typeof setTimeout>;
  return Promise.race([
    value,
    new Promise<never>((_, reject) => {
      timer = setTimeout(
        () => reject(new Error(`ACK case timed out: ${step}`)),
        30_000,
      );
    }),
  ]).finally(() => clearTimeout(timer));
}

// verifies: PROC-040, PROC-052
export async function readerAckFault(backend: sdk.BackendOptions) {
  for (const atCommit of [false, true]) {
    for (const scope of ["group", "all"] as const) {
      await runFault(backend, scope, atCommit, true);
    }
    await runFault(backend, "group", atCommit, false);
  }
  console.log(
    "Node ACK write/commit faults, callback scopes and replay passed",
  );
}

async function runFault(
  backend: sdk.BackendOptions,
  scope: Scope,
  atCommit: boolean,
  callback: boolean,
) {
  const client = await sdk.Client.create(await sdk.generateLocalSigner(), {
    backend,
    storage: { location: "inMemory", singleConnection: true },
    deviceSync: false,
  });
  const reasons: sdk.StreamCloseReason[] = [];
  const received: string[] = [];
  let faultInstalled = false;
  let stream: sdk.MessageStream | undefined;
  const caseName = `${scope}/${atCommit ? "commit" : "write"}/${callback ? "callback" : "iterator"}`;
  console.log(`Node ACK case ${caseName}: start`);
  try {
    const group = await client.conversations.createGroup([]);
    const firstId = await group.sendText("retained after ACK failure");
    const before = await client.conversations.sdkConformanceDeliveryPosition(
      group.id,
    );
    const open = (onClose?: (reason: sdk.StreamCloseReason) => void) =>
      scope === "group"
        ? sdk.MessageStream.openGroup(client, group, undefined, { onClose })
        : sdk.MessageStream.open(client, undefined, { onClose });
    stream = open((reason) => reasons.push(reason));
    console.log(`Node ACK case ${caseName}: stream ready`);
    await bounded(stream.ready(), `${caseName} ready`);
    let failure: unknown;
    if (callback) {
      try {
        await bounded(
          stream.onValue(async (message) => {
            received.push(message.id);
            assert.equal(message.id, firstId);
            console.log(`Node ACK case ${caseName}: first callback`);
            await client.conversations.sdkConformanceInstallAckFailure(
              atCommit,
            );
            faultInstalled = true;
          }),
          `${caseName} callback fault`,
        );
      } catch (error) {
        failure = error;
      }
    } else {
      const first = await bounded(stream.next(), `${caseName} first read`);
      assert.equal(first.value?.id, firstId);
      assert.ok(first.value);
      received.push(first.value.id);
      console.log(`Node ACK case ${caseName}: first iterator value`);
      await client.conversations.sdkConformanceInstallAckFailure(atCommit);
      faultInstalled = true;
      try {
        await bounded(stream.next(), `${caseName} iterator fault`);
      } catch (error) {
        failure = error;
      }
    }
    console.log(`Node ACK case ${caseName}: fault returned`);
    assert.equal(faultInstalled, true);
    assert.ok(failure instanceof sdk.XmtpError.Storage, String(failure));
    assert.equal(failure.details.code, "Storage");
    assert.equal(failure.details.category, "storage");
    assert.ok(failure.details.message.length > 0);
    assert.deepEqual(received, [firstId]);
    assert.equal(reasons.length, 1);
    assert.equal(reasons[0]?.kind, "failed");
    if (reasons[0]?.kind === "failed") assert.equal(reasons[0].error, failure);
    await client.conversations.sdkConformanceClearAckFailure();
    faultInstalled = false;
    assert.equal(
      await client.conversations.sdkConformanceDeliveryPosition(group.id),
      before,
      "failed ACK advanced the durable cursor",
    );
    const secondId = await group.sendText("after retained delivery");
    const replacement = open();
    try {
      assert.equal(
        (await bounded(replacement.next(), `${caseName} replay first`)).value
          ?.id,
        firstId,
      );
      assert.equal(
        (await bounded(replacement.next(), `${caseName} replay second`)).value
          ?.id,
        secondId,
      );
    } finally {
      await replacement.end();
    }
    await stream.end();
    assert.equal(reasons.length, 1);
    console.log(
      `Node ACK case ${caseName}: typed failure, one close, unchanged cursor, and replay passed`,
    );
  } finally {
    await stream?.end();
    if (faultInstalled)
      await client.conversations.sdkConformanceClearAckFailure();
    await client.end();
  }
}
