import "../../../../target/sdk-conformance/typescript-napi/binding.ts";
import assert from "node:assert/strict";

import * as B from "../../../../target/sdk-conformance/typescript-napi/xmtp_sdk.ts";
import {
  dispose,
  drained,
  heldCounts,
  foreignTasksDrained,
  lifetimeCalls,
  lifetimeCycles,
  rawOptions,
  signal,
  within,
} from "./callback-lifetime-support.mts";
import { checkNodeCallbackOperations } from "./node-callback-operations.mts";
import {
  checkConstructorAdoption,
  checkConstructorFailure,
} from "./node-constructor-lifetime.mts";
import { checkNodeCredentialLifetime } from "./node-credential-lifetime.mts";
import { checkNodeCredentialOwnership } from "./node-credential-ownership.mts";
import { checkNodeStreamLifetime } from "./node-stream-lifetime.mts";

type Family = "identity" | "kind" | "sign" | "preAuthenticate";

async function creationCycle(backendURL: string, family: Family) {
  const width = family === "preAuthenticate" ? 1 : lifetimeCalls;
  const allEntered = signal();
  const action = signal();
  const reentered = signal();
  const release = signal();
  const finished = signal();
  let entered = 0;
  let active = 0;
  let ended = 0;
  let completed = 0;
  const independentSigner = await B.generateLocalSigner();
  const independent = await B.Client.create(
    independentSigner,
    rawOptions(backendURL),
  );
  const controllers: AbortController[] = [];
  const calls: Promise<PromiseSettledResult<B.ClientLike>>[] = [];
  const signers: B.Signer[] = [];
  async function hold() {
    entered++;
    active++;
    if (entered === width) allEntered.resolve();
    try {
      await action.promise;
      await independent.end();
      ended++;
      if (ended === width) reentered.resolve();
      await release.promise;
    } finally {
      active--;
      completed++;
      if (completed === width) finished.resolve();
    }
  }
  try {
    for (let index = 0; index < width; index++) {
      const base = await B.generateLocalSigner();
      signers.push(base);
      const signer: B.Signer = {
        async identity() {
          if (family === "identity") await hold();
          return base.identity();
        },
        async kind() {
          if (family === "kind") await hold();
          return base.kind();
        },
        async sign(request) {
          if (family === "sign") await hold();
          return base.sign(request);
        },
      };
      const options = rawOptions(backendURL);
      if (family === "preAuthenticate")
        options.handlers = { preAuthenticate: { run: hold } };
      const abort = new AbortController();
      controllers.push(abort);
      calls.push(
        B.Client.create(signer, options, { signal: abort.signal }).then(
          (value) => ({ status: "fulfilled", value }) as const,
          (reason: unknown) => ({ status: "rejected", reason }) as const,
        ),
      );
    }
    await within(allEntered.promise, `${family} entry`);
    assert.equal(active, width);
    heldCounts(width);
    for (const controller of controllers) controller.abort();
    const cancelled = await within(
      Promise.all(calls),
      `${family} caller cancellation`,
    );
    for (const result of cancelled) {
      if (result.status === "fulfilled") {
        await result.value.end();
        dispose(result.value);
      }
      assert.equal(
        result.status,
        "rejected",
        "cancelled create returned a client",
      );
    }
    assert.equal(active, width, "caller cancellation ended a host callback");
    heldCounts(width);
    action.resolve();
    await within(reentered.promise, `${family} independent client end`);
    assert.equal(active, width);
    release.resolve();
    await within(finished.promise, `${family} callback release`);
  } finally {
    action.resolve();
    release.resolve();
    for (const controller of controllers) controller.abort();
    await independent.end();
    await foreignTasksDrained();
    dispose(independent);
    for (const signer of signers) dispose(signer);
    dispose(independentSigner);
  }
  await drained();
}

export async function checkNodeCallbackLifetime(backendURL: string) {
  const selected = process.env.SDK_CALLBACK_LIFETIME_FAMILY;
  if (selected === "credentialOwnership") {
    await checkNodeCredentialOwnership(backendURL);
    return;
  }
  for (const family of [
    "identity",
    "kind",
    "sign",
    "preAuthenticate",
  ] as const) {
    if (selected && selected !== family) continue;
    for (let cycle = 0; cycle < lifetimeCycles; cycle++)
      await creationCycle(backendURL, family);
    console.log(
      `Node callback lifetime: ${family}, ${lifetimeCycles} cycles passed`,
    );
  }
  await checkNodeStreamLifetime(backendURL, selected);
  if (!selected || selected === "credential")
    await checkNodeCredentialLifetime(backendURL);
  await checkNodeCallbackOperations(backendURL, selected);
  if (!selected || selected === "constructorFailure")
    await checkConstructorFailure(backendURL);
  if (!selected || selected === "adoption")
    await checkConstructorAdoption(backendURL);
  console.log(
    JSON.stringify({
      host: "node",
      source: process.env.SDK_CALLBACK_SOURCE_SHA,
      memory: process.memoryUsage(),
      resource: process.resourceUsage(),
    }),
  );
}
