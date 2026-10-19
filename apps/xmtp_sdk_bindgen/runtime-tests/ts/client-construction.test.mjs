import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { stripTypeScriptTypes } from "node:module";

import { test } from "vitest";

const dataModule = (source) =>
  `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
// Node imports the isolated dependency graph directly. The entire production
// wrapper and codec implementation run unchanged apart from type erasure.
const importModule = (url) => import(/* @vite-ignore */ url);
const rawUrl = dataModule(`
export const state = { allocated: 0, retained: 0, released: 0, discarded: 0 };
export const Client = {
  async create() { return allocate(); },
  async build() { return allocate(); },
};
function allocate() {
  state.allocated += 1;
  return { clientKey() { if (state.keyError) throw state.keyError; return 1n; } };
}
export class ListenerError {}
export const ErrorCategory = { Callback: "callback" };
`);
const platformUrl = dataModule(`
import { state } from ${JSON.stringify(rawUrl)};
export const resolvedOptions = value => value;
export function retainPlatformClient() { state.retained += 1; }
export function releasePlatformClient() { state.released += 1; }
export async function discardPlatformClient() {
  state.discarded += 1;
  if (state.discardError) throw state.discardError;
}
`);
function production(path, imports) {
  let source = stripTypeScriptTypes(readFileSync(path, "utf8"));
  for (const [specifier, url] of Object.entries(imports)) {
    source = source.replaceAll(JSON.stringify(specifier), JSON.stringify(url));
  }
  return dataModule(source);
}
const codecUrl = production(
  "apps/xmtp_sdk_bindgen/runtime/ts/custom-codec.ts",
  { "../xmtp_sdk": rawUrl },
);
const clientUrl = production("apps/xmtp_sdk_bindgen/runtime/ts/client.ts", {
  "../xmtp_sdk": rawUrl,
  "../client-forwarding.gen": dataModule("export class ClientForwarders {}"),
  "./custom-codec": codecUrl,
  "./events/reader": dataModule("export class EventStream {}"),
  "./platform": platformUrl,
});
const { Client } = await importModule(clientUrl);
const { state } = await importModule(rawUrl);
function reset() {
  Object.assign(state, {
    allocated: 0,
    retained: 0,
    released: 0,
    discarded: 0,
    keyError: undefined,
    discardError: undefined,
  });
}

test("throwing codec getters reject before either constructor allocates a native Client", async () => {
  for (const method of ["create", "build"]) {
    reset();
    const error = new Error("codec getter failed");
    const codec = {
      get type() {
        throw error;
      },
    };
    await assert.rejects(
      Client[method]({}, { codecs: [codec] }),
      (value) => value === error,
    );
    assert.equal(state.allocated, 0);
    assert.equal(state.retained, 0);
  }
});

test("wrapper failure discards the native Client and releases its owner exactly once", async () => {
  for (const method of ["create", "build"]) {
    for (const cleanupFails of [false, true]) {
      reset();
      const error = new Error("client key failed");
      state.keyError = error;
      state.discardError = cleanupFails
        ? new Error("cleanup failed")
        : undefined;
      await assert.rejects(Client[method]({}, {}), (value) => value === error);
      assert.equal(state.allocated, 1);
      assert.equal(state.retained, 1);
      assert.equal(state.discarded, 1);
      assert.equal(state.released, 1);
    }
  }
});
