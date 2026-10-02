/* eslint-disable typescript/prefer-promise-reject-errors -- Test non-Error callback rejections. */
import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";
import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";

import {
  Client,
  XmtpError,
  generateLocalSigner,
  initLogging,
  setLogSink,
  localSignerFromPrivateKey,
} from "@xmtp/node-sdk";
const require = createRequire(import.meta.url);
const packageRoot = dirname(require.resolve("@xmtp/node-sdk/package.json"));
const bindingRoot = existsSync(join(packageRoot, "xmtp_sdk.js"))
  ? packageRoot
  : join(packageRoot, "dist");
const B = await import(pathToFileURL(join(bindingRoot, "xmtp_sdk.js")));
const createIdentifier = () => ({
  kind: "ethereum",
  identifier: `0x${randomBytes(20).toString("hex")}`,
});
async function createSigner() {
  const base = await generateLocalSigner();
  return {
    identity: () => base.identity(),
    kind: () => base.kind(),
    sign: (request) => base.sign(request),
  };
}
const backend = { url: process.env.XMTP_BACKEND_URL };
const options = {
  backend,
  storage: { location: "inMemory" },
  deviceSync: false,
};
const privateReason = "callback-private-rejection-sentinel";
const failures = [];
const unhandled = [];
process.on("unhandledRejection", (reason) => {
  unhandled.push(reason);
  console.error("UNHANDLED", reason);
});
async function bounded(call, label) {
  let timer;
  try {
    return await Promise.race([
      call,
      new Promise((_, reject) => {
        timer = setTimeout(
          () => reject(new Error(`${label} did not settle`)),
          5000,
        );
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}
async function eventually(predicate, label) {
  await bounded(
    (async () => {
      while (!predicate())
        await new Promise((resolve) => setTimeout(resolve, 10));
    })(),
    label,
  );
}
let checks = 0;
async function check(name, action) {
  checks++;
  try {
    await action();
    console.log(name, "PASS");
  } catch (error) {
    failures.push(name);
    console.error(name, "FAIL", error);
  }
}
function assertFailure(error, code, retryable) {
  assert.equal(error.details.code, code);
  assert.equal(error.details.category, "callback");
  assert.equal(error.details.retryable, retryable);
  assert.ok(!JSON.stringify(error).includes(privateReason));
  assert.ok(!("cause" in error));
}
async function typedIdentity(name, factory, action) {
  const declared = factory();
  const converter = B.default.converters[`FfiConverterType${name}`];
  const original = converter.lower;
  let seen = 0;
  converter.lower = function (value, ...args) {
    assert.equal(
      value,
      declared,
      `${name} declared rejection identity changed`,
    );
    seen++;
    return original.call(this, value, ...args);
  };
  try {
    await action(declared);
    assert.ok(seen > 0, `${name} did not use its declared error converter`);
  } finally {
    converter.lower = original;
  }
}
for (const reason of [
  null,
  undefined,
  new Error(privateReason),
  { kind: "failed" },
]) {
  const label =
    reason === null
      ? "null"
      : reason === undefined
        ? "undefined"
        : reason instanceof Error
          ? "Error"
          : "public tagged record";
  await check(`credential/${label}`, async () => {
    const error = await bounded(
      Client.canMessage([createIdentifier()], {
        ...backend,
        credentials: { credential: () => Promise.reject(reason) },
      }),
      "credential",
    ).catch((error) => error);
    assertFailure(error, "CredentialCallbackFailed", true);
  });
}
await check("credential/declared binding identity", () =>
  typedIdentity(
    "CredentialError",
    () => B.CredentialError.Failed.new(),
    async (reason) => {
      const error = await bounded(
        Client.canMessage([createIdentifier()], {
          ...backend,
          credentials: { credential: () => Promise.reject(reason) },
        }),
        "declared credential",
      ).catch((error) => error);
      assertFailure(error, "CredentialCallbackFailed", true);
    },
  ),
);
for (const method of ["identity", "kind", "sign"]) {
  for (const reason of [null, undefined, new Error(privateReason)]) {
    await check(
      `signer/${method}/${reason === null ? "null" : reason === undefined ? "undefined" : "Error"}`,
      async () => {
        const signer = await createSigner();
        signer[method] = () => Promise.reject(reason);
        const error = await bounded(
          Client.create(signer, options),
          `signer ${method}`,
        ).catch((error) => error);
        assert.ok(error instanceof XmtpError.Signer);
        assertFailure(error, "SignerFailed", false);
      },
    );
  }
  await check(`signer/${method}/declared binding identity`, () =>
    typedIdentity(
      "SignerError",
      () => B.SignerError.Failed.new(),
      async (reason) => {
        const signer = await createSigner();
        signer[method] = () => Promise.reject(reason);
        const error = await bounded(
          Client.create(signer, options),
          `declared signer ${method}`,
        ).catch((error) => error);
        assertFailure(error, "SignerFailed", false);
      },
    ),
  );
}
for (const reason of [null, undefined, new Error(privateReason)]) {
  await check(
    `preAuthenticate/${reason === null ? "null" : reason === undefined ? "undefined" : "Error"}`,
    async () => {
      const error = await bounded(
        Client.create(await createSigner(), {
          ...options,
          handlers: { preAuthenticate: { run: () => Promise.reject(reason) } },
        }),
        "preAuthenticate",
      ).catch((error) => error);
      assert.ok(error instanceof XmtpError.CallbackFailed);
      assertFailure(error, "CallbackFailed", false);
    },
  );
}
await check("preAuthenticate/declared binding identity", () =>
  typedIdentity(
    "PreAuthenticateError",
    () => B.PreAuthenticateError.Failed.new(),
    async (reason) => {
      const error = await bounded(
        Client.create(await createSigner(), {
          ...options,
          handlers: { preAuthenticate: { run: () => Promise.reject(reason) } },
        }),
        "declared preAuthenticate",
      ).catch((error) => error);
      assert.ok(error instanceof XmtpError.CallbackFailed);
    },
  ),
);
async function eventContinuation(reason) {
  const client = await bounded(
    Client.create(await createSigner(), options),
    "event owner",
  );
  let calls = 0;
  let id;
  try {
    id = await client.startListener(
      { kinds: ["conversationJoined"], referencesOwnMessages: false },
      () => {
        calls++;
        return calls === 1 ? Promise.reject(reason) : Promise.resolve();
      },
    );
    await client.conversations.createGroup([]);
    await eventually(() => calls === 1, "first event callback");
    await client.conversations.createGroup([]);
    await eventually(() => calls === 2, "later event callback");
  } finally {
    if (id !== undefined)
      await bounded(client.stopListener(id), "listener stop");
    await bounded(client.end(), "event owner end");
  }
}
for (const reason of [null, undefined, new Error(privateReason)])
  await check(
    `event/${reason === null ? "null" : reason === undefined ? "undefined" : "Error"}/continuation`,
    () => eventContinuation(reason),
  );
async function rawEventContinuation(reason) {
  const signer = await B.generateLocalSigner();
  const client = await bounded(
    B.Client.create(
      signer,
      B.ClientOptions.new({
        backend: B.BackendSource.Options.new({
          options: B.BackendOptions.new(backend),
        }),
        storage: B.StorageOptions.new({
          location: B.StorageLocation.InMemory.new(),
        }),
        deviceSync: false,
      }),
    ),
    "raw event owner",
  );
  const conversations = client.conversations();
  let calls = 0;
  let id;
  try {
    id = await client.startListener(
      { kinds: [B.EventKind.ConversationJoined], referencesOwnMessages: false },
      {
        onEvent: () => {
          calls++;
          return calls === 1 ? Promise.reject(reason) : Promise.resolve();
        },
      },
    );
    await conversations.createGroup([]);
    await eventually(() => calls === 1, "first raw event callback");
    await conversations.createGroup([]);
    await eventually(() => calls === 2, "later raw event callback");
  } finally {
    if (id !== undefined)
      await bounded(client.stopListener(id), "raw listener stop");
    await bounded(client.end(), "raw event owner end");
    conversations.uniffiDestroy();
    client.uniffiDestroy();
    signer.uniffiDestroy();
  }
}
for (const reason of [null, undefined, new Error(privateReason)])
  await check(
    `raw event/${reason === null ? "null" : reason === undefined ? "undefined" : "Error"}/continuation`,
    () => rawEventContinuation(reason),
  );
await check("event/declared binding identity", () =>
  typedIdentity(
    "ListenerError",
    () => B.ListenerError.Failed.new(),
    rawEventContinuation,
  ),
);
await initLogging({ level: "error", structured: true });
async function logContinuation(reason) {
  let calls = 0;
  try {
    await setLogSink({
      log: () => {
        calls++;
        return calls === 1 ? Promise.reject(reason) : Promise.resolve();
      },
    });
    await assert.rejects(localSignerFromPrivateKey(new Uint8Array(31)));
    await eventually(() => calls === 1, "first log callback");
    await assert.rejects(localSignerFromPrivateKey(new Uint8Array(31)));
    await eventually(() => calls >= 2, "later log callback");
  } finally {
    await bounded(setLogSink(), "log sink clear");
  }
}
for (const reason of [null, undefined, new Error(privateReason)])
  await check(
    `log/${reason === null ? "null" : reason === undefined ? "undefined" : "Error"}/continuation`,
    () => logContinuation(reason),
  );
async function rawLogContinuation(reason) {
  let calls = 0;
  try {
    await B.setLogSink({
      log: () => {
        calls++;
        return calls === 1 ? Promise.reject(reason) : Promise.resolve();
      },
    });
    await assert.rejects(
      B.localSignerFromPrivateKey(new Uint8Array(31).buffer),
    );
    await eventually(() => calls === 1, "first raw log callback");
    await assert.rejects(
      B.localSignerFromPrivateKey(new Uint8Array(31).buffer),
    );
    await eventually(() => calls >= 2, "later raw log callback");
  } finally {
    await bounded(B.setLogSink(), "raw log sink clear");
  }
}
for (const reason of [null, undefined, new Error(privateReason)])
  await check(
    `raw log/${reason === null ? "null" : reason === undefined ? "undefined" : "Error"}/continuation`,
    () => rawLogContinuation(reason),
  );
await check("log/declared binding identity", () =>
  typedIdentity(
    "LogSinkError",
    () => B.LogSinkError.Failed.new({ reason: "declared failure" }),
    rawLogContinuation,
  ),
);
assert.equal(
  unhandled.length,
  0,
  "callback normalization left unhandled rejections",
);
console.log(
  JSON.stringify({
    result: failures.length ? "FAIL" : "PASS",
    checks,
    failures,
    unhandled: unhandled.length,
    node: process.version,
  }),
);
process.exit(failures.length ? 1 : 0);
