import assert from "node:assert/strict";
import { setImmediate as nextTurn } from "node:timers/promises";

import * as B from "../../../../target/sdk-conformance/typescript-napi/xmtp_sdk.ts";
import {
  dispose,
  drained,
  heldCounts,
  lifetimeCalls,
  lifetimeCycles,
  rawOptions,
  signal,
  within,
} from "./callback-lifetime-support.mts";

async function eventCycle(backendURL: string, ownEnd: boolean) {
  const signer = await B.generateLocalSigner();
  const client = await B.Client.create(signer, rawOptions(backendURL));
  const entered = signal();
  const action = signal();
  const reentered = signal();
  const release = signal();
  const finished = signal();
  let calls = 0;
  let active = 0;
  let id = 0n;
  try {
    id = await client.startListener(
      { kinds: [B.EventKind.HmacKeysUpdated], referencesOwnMessages: false },
      {
        async onEvent() {
          calls++;
          active++;
          entered.resolve();
          try {
            await action.promise;
            if (ownEnd) await client.end();
            else await client.stopListener(id);
            reentered.resolve();
            await release.promise;
          } finally {
            active--;
            finished.resolve();
          }
        },
      },
    );
    client.sdkConformanceEmitHmacEvents(1);
    await within(entered.promise, "event callback entry");
    client.sdkConformanceEmitHmacEvents(1030);
    assert.deepEqual(client.sdkConformanceListenerCounts(id), {
      registered: true,
      queued: 1023n,
      inFlight: 1n,
      discarded: 7n,
    });
    assert.equal(calls, 1, "serial callback handed off queued work while held");
    heldCounts(1);
    action.resolve();
    await within(reentered.promise, "event callback stop/end reentry");
    assert.equal(active, 1, "stop/end dropped the active event callback");
    assert.equal(calls, 1);
    assert.equal(client.sdkConformanceListenerCounts(id).registered, false);
    release.resolve();
    await within(finished.promise, "event callback release");
    await nextTurn();
    assert.equal(calls, 1, "queued event handed off after stop/end");
  } finally {
    action.resolve();
    release.resolve();
    await client.stopListener(id);
    await client.end();
    dispose(client);
    dispose(signer);
  }
  await drained();
}

async function signatureCycle(backendURL: string) {
  const entered = signal();
  const release = signal();
  const clients: B.ClientLike[] = [];
  const requests: B.SignatureRequestLike[] = [];
  const signers: B.Signer[] = [];
  const calls: Promise<void>[] = [];
  let active = 0;
  let total = 0;
  try {
    for (let index = 0; index < lifetimeCalls; index++) {
      const base = await B.generateLocalSigner();
      signers.push(base);
      const options = rawOptions(backendURL);
      options.registration = { auto: false };
      const client = await B.Client.create(base, options);
      clients.push(client);
      const request = await client.unsafeCreateInboxSignatureRequest();
      assert.ok(request);
      requests.push(request);
      calls.push(
        request.sign({
          identity: () => base.identity(),
          kind: () => base.kind(),
          async sign(value) {
            active++;
            total++;
            if (total === lifetimeCalls) entered.resolve();
            try {
              await release.promise;
              return await base.sign(value);
            } finally {
              active--;
            }
          },
        }),
      );
    }
    await within(entered.promise, "signature request callback entry");
    heldCounts(lifetimeCalls);
    await within(
      Promise.all(clients.map((client) => client.end())),
      "signature owners end",
    );
    assert.equal(active, lifetimeCalls);
    release.resolve();
    await within(Promise.all(calls), "signature callback release");
    assert.equal(active, 0);
    for (let index = 0; index < clients.length; index++) {
      await assert.rejects(
        clients[index].unsafeApplySignatureRequest(requests[index]),
        (error) => error instanceof B.XmtpError.ClientClosed,
      );
    }
  } finally {
    release.resolve();
    await Promise.allSettled(calls);
    for (const client of clients) {
      await client.end();
      dispose(client);
    }
    for (const request of requests) dispose(request);
    for (const signer of signers) dispose(signer);
  }
  await drained();
}

export async function checkNodeCallbackOperations(
  backendURL: string,
  selected?: string,
) {
  for (const family of ["eventStop", "eventEnd", "signatureRequest"] as const) {
    if (selected && selected !== family) continue;
    for (let cycle = 0; cycle < lifetimeCycles; cycle++) {
      if (family === "signatureRequest") await signatureCycle(backendURL);
      else await eventCycle(backendURL, family === "eventEnd");
    }
    console.log(
      `Node callback lifetime: ${family}, ${lifetimeCycles} cycles passed`,
    );
  }
}
