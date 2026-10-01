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

// The callback can end an independent client. Awaiting its own client's end
// is unsupported: that end waits for the callback's foreground call.
async function credentialCycle(backendURL: string) {
  const entered = signal();
  const action = signal();
  const reentered = signal();
  const release = signal();
  const finished = signal();
  const independentSigner = await B.generateLocalSigner();
  const independent = await B.Client.create(
    independentSigner,
    rawOptions(backendURL),
  );
  const clients: B.ClientLike[] = [];
  const groups: B.GroupLike[] = [];
  const signers: B.Signer[] = [];
  const controllers: AbortController[] = [];
  const calls: Promise<PromiseSettledResult<void>>[] = [];
  let active = 0;
  let entries = 0;
  let reentries = 0;
  let completions = 0;
  try {
    for (let index = 0; index < lifetimeCalls; index++) {
      let armed = false;
      const options = rawOptions(backendURL);
      options.backend = B.BackendSource.Options.new({
        options: {
          url: backendURL,
          credentials: {
            async credential() {
              if (armed) {
                active++;
                entries++;
                if (entries === lifetimeCalls) entered.resolve();
                try {
                  await action.promise;
                  await independent.end();
                  if (++reentries === lifetimeCalls) reentered.resolve();
                  await release.promise;
                } finally {
                  active--;
                  if (++completions === lifetimeCalls) finished.resolve();
                }
              }
              return {
                value: "Bearer callback-lifetime",
                expiresAtSeconds: 9_000_000_000_000_000n,
              };
            },
          },
        },
      });
      const signer = await B.generateLocalSigner();
      signers.push(signer);
      const client = await B.Client.create(signer, options);
      clients.push(client);
      const conversations = client.conversations();
      const group = await conversations.createGroup([], undefined);
      dispose(conversations);
      groups.push(group);
      await client.setCredential({
        value: "Bearer expired",
        expiresAtSeconds: 0n,
      });
      armed = true;
      const controller = new AbortController();
      controllers.push(controller);
      calls.push(
        group.sync({ signal: controller.signal }).then(
          (value) => ({ status: "fulfilled", value }) as const,
          (reason: unknown) => ({ status: "rejected", reason }) as const,
        ),
      );
    }
    await within(entered.promise, "credential renewal callback entry");
    assert.equal(active, lifetimeCalls);
    heldCounts(lifetimeCalls);
    for (const controller of controllers) controller.abort();
    const outcomes = await within(
      Promise.all(calls),
      "credential caller cancellation",
    );
    outcomes.forEach((outcome) => assert.equal(outcome.status, "rejected"));
    assert.equal(
      active,
      lifetimeCalls,
      "caller cancellation ended credential callback",
    );
    action.resolve();
    await within(reentered.promise, "credential independent end reentry");
    let endCompleted = false;
    const ends = Promise.all(clients.map((client) => client.end())).then(() => {
      endCompleted = true;
    });
    await nextTurn();
    assert.equal(active, lifetimeCalls);
    assert.equal(
      endCompleted,
      false,
      "owner end skipped held foreground callbacks",
    );
    release.resolve();
    await within(finished.promise, "credential callback release");
    await within(ends, "credential owner end");
    for (const group of groups)
      await assert.rejects(
        group.sync(),
        (error) => error instanceof B.XmtpError.ClientClosed,
      );
  } finally {
    action.resolve();
    release.resolve();
    controllers.forEach((controller) => controller.abort());
    await independent.end();
    dispose(independent);
    dispose(independentSigner);
    for (const client of clients) {
      await client.end();
      dispose(client);
    }
    groups.forEach(dispose);
    signers.forEach(dispose);
  }
  await drained();
}

export async function checkNodeCredentialLifetime(backendURL: string) {
  for (let cycle = 0; cycle < lifetimeCycles; cycle++)
    await credentialCycle(backendURL);
  console.log(
    `Node callback lifetime: credential, ${lifetimeCycles} cycles passed`,
  );
}
