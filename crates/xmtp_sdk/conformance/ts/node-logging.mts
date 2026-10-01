import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { realpathSync } from "node:fs";
import { fileURLToPath } from "node:url";

import * as sdk from "../../../../target/sdk-conformance/typescript-napi/index.ts";
import { waitForLog } from "./logging-wait.js";

export async function logging(
  reopened: sdk.Client,
  snapshot: sdk.ServerConfiguration,
): Promise<void> {
  await sdk.initLogging({
    level: "error",
    structured: true,
    performance: false,
    otel: undefined,
    resourceAttributes: new Map(),
  });
  let sinkDelivered!: () => void;
  const sinkRecord = new Promise<void>((resolve) => {
    sinkDelivered = resolve;
  });
  let sinkError: unknown;
  await sdk.setLogSink({
    async log(record) {
      try {
        assert.ok(record.target.length > 0);
        assert.equal(typeof record.level, "string");
        assert.ok(record.fields instanceof Map);
        assert.equal(typeof record.droppedRecords, "bigint");
        assert.equal(
          reopened.serverConfiguration.identifier,
          snapshot.identifier,
        );
      } catch (error) {
        sinkError = error;
      }
      sinkDelivered();
    },
  });
  await assert.rejects(sdk.localSignerFromPrivateKey(new Uint8Array(31)));
  await waitForLog(sinkRecord, "queued log sink did not run");
  await sdk.setLogSink(undefined);
  if (sinkError !== undefined) throw sinkError;
  console.log("Node logging: queued sink called Rust without a deadlock");
  let sinkThrew = false;
  let rejected!: () => void;
  const rejection = new Promise<void>((resolve) => {
    rejected = resolve;
  });
  await sdk.setLogSink({
    async log() {
      sinkThrew = true;
      rejected();
      throw new Error("test sink failure");
    },
  });
  await assert.rejects(sdk.localSignerFromPrivateKey(new Uint8Array(31)));
  await waitForLog(rejection, "failing log sink did not run");
  await sdk.setLogSink(undefined);
  assert.equal(sinkThrew, true, "failing sink was not called");
  assert.match(sdk.sdkVersion(), /^1\.12\.0/);
  console.log("Node logging: sink error did not stop the process");

  // setLogSink() returns a Promise on Node, as in the browser, and clears the
  // installed sink.
  let clearedSinkCalls = 0;
  await sdk.setLogSink({
    async log() {
      clearedSinkCalls += 1;
    },
  });
  const cleared = sdk.setLogSink();
  assert.ok(
    cleared instanceof Promise,
    "setLogSink() did not return a Promise",
  );
  await cleared;
  const callsAtClear = clearedSinkCalls;
  await assert.rejects(sdk.localSignerFromPrivateKey(new Uint8Array(31)));
  assert.equal(clearedSinkCalls, callsAtClear, "a cleared sink was called");
  console.log(
    "Node logging: setLogSink() returns a Promise and clears the sink",
  );

  const loggingChild = fileURLToPath(
    new URL("./logging-child.mts", import.meta.url),
  );
  await new Promise<void>((resolve, reject) => {
    const child = spawn(
      process.execPath,
      [
        "--import",
        realpathSync(
          fileURLToPath(
            new URL(
              "../../../../sdks/node/node_modules/tsx/dist/loader.mjs",
              import.meta.url,
            ),
          ),
        ),
        loggingChild,
      ],
      { env: process.env, stdio: "inherit" },
    );
    const timeout = setTimeout(() => {
      child.kill("SIGKILL");
      reject(new Error("async logging child did not complete"));
    }, 30_000);
    child.on("error", (error) => {
      clearTimeout(timeout);
      reject(error);
    });
    child.on("exit", (code) => {
      clearTimeout(timeout);
      if (code === 0) resolve();
      else reject(new Error(`logging child exited with ${code}`));
    });
  });
  console.log("Node logging: queued sink avoided the lock inversion");
}

// verifies: LOG-008
export async function loggingEnd(client: sdk.Client): Promise<void> {
  let resolve!: () => void;
  let reject!: (error: unknown) => void;
  const ended = new Promise<void>((ok, fail) => {
    resolve = ok;
    reject = fail;
  });
  await sdk.setLogSink({
    async log() {
      try {
        await client.end();
        await sdk.setLogSink();
        resolve();
      } catch (error) {
        reject(error);
      }
    },
  });
  await sdk.sdkConformanceEmit(1);
  await waitForLog(ended, "log callback end did not complete");
  await assert.rejects(
    client.isRegistered(),
    (error) => error instanceof sdk.XmtpError.ClientClosed,
  );
}

// verifies: LOG-010
export async function loggingSecrets(): Promise<void> {
  const credential = "LOG_CREDENTIAL_SENTINEL_89d42";
  const signing = new TextEncoder().encode("LOG_SIGNING_KEY_SENTINEL_89d42!!!");
  const database = new TextEncoder().encode("LOG_DATABASE_KEY_SENTINEL_89d42");
  const forbidden = [
    credential,
    ...[signing, database].flatMap((bytes) => [
      new TextDecoder().decode(bytes),
      Buffer.from(bytes).toString("hex"),
      `[${[...bytes].join(", ")}]`,
    ]),
  ];
  const logs: string[] = [];
  let done!: () => void;
  const barrier = new Promise<void>((resolve) => {
    done = resolve;
  });
  await sdk.setLogSink({
    async log(record) {
      logs.push(record.message, ...record.fields.values());
      if (record.target === "xmtp_sdk::conformance") done();
    },
  });
  await assert.rejects(
    sdk.Backend.connect({
      url: "http://127.0.0.1:1",
      credentials: {
        value: `Bearer ${credential}\n`,
        expiresAtSeconds: 0n,
      },
    }),
  );
  await assert.rejects(sdk.localSignerFromPrivateKey(signing));
  await assert.rejects(
    sdk.Client.create(await sdk.generateLocalSigner(), {
      backend: { url: process.env.XMTP_BACKEND_URL! },
      deviceSync: false,
      storage: { location: "inMemory", encryptionKey: database },
    }),
  );
  await sdk.sdkConformanceEmit(1);
  await waitForLog(barrier, "secret log barrier did not run");
  await sdk.setLogSink();
  for (const secret of forbidden)
    assert.ok(
      !logs.some((log) => log.includes(secret)),
      "app log exposed a secret",
    );
}
