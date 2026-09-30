import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { realpathSync } from "node:fs";
import { fileURLToPath } from "node:url";

import * as sdk from "../../../../target/sdk-conformance/typescript-napi/index.ts";

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
  sdk.setLogSink({
    log(record) {
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
  await Promise.race([
    sinkRecord,
    new Promise<never>((_, reject) =>
      setTimeout(() => reject(new Error("queued log sink did not run")), 3_000),
    ),
  ]);
  sdk.setLogSink(undefined);
  if (sinkError !== undefined) throw sinkError;
  console.log("Node logging: queued sink called Rust without a deadlock");
  let sinkThrew = false;
  sdk.setLogSink({
    log() {
      sinkThrew = true;
      throw new Error("test sink failure");
    },
  });
  await assert.rejects(sdk.localSignerFromPrivateKey(new Uint8Array(31)));
  for (let attempt = 0; attempt < 30 && !sinkThrew; attempt += 1) {
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  sdk.setLogSink(undefined);
  assert.equal(sinkThrew, true, "failing sink was not called");
  assert.match(sdk.sdkVersion(), /^1\.12\.0/);
  console.log("Node logging: sink error did not stop the process");

  // clearLogSink returns a Promise on Node, as in the browser, and clears the
  // installed sink.
  let clearedSinkCalls = 0;
  sdk.setLogSink({
    log() {
      clearedSinkCalls += 1;
    },
  });
  const cleared = sdk.clearLogSink();
  assert.ok(cleared instanceof Promise, "clearLogSink did not return a Promise");
  await cleared;
  const callsAtClear = clearedSinkCalls;
  await assert.rejects(sdk.localSignerFromPrivateKey(new Uint8Array(31)));
  await new Promise((resolve) => setTimeout(resolve, 50));
  assert.equal(clearedSinkCalls, callsAtClear, "a cleared sink was called");
  console.log("Node logging: clearLogSink returns a Promise and clears the sink");

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
      reject(new Error("inline log sink deadlocked while Rust held a lock"));
    }, 5_000);
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

  let droppedRecords = 0n;
  let firstRecord = true;
  sdk.setLogSink({
    log(record) {
      if (firstRecord) {
        firstRecord = false;
        Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 500);
      }
      if (record.droppedRecords > droppedRecords)
        droppedRecords = record.droppedRecords;
    },
  });
  await sdk.sdkConformanceEmit(10_000);
  for (let attempt = 0; attempt < 100 && droppedRecords === 0n; attempt += 1) {
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  sdk.setLogSink(undefined);
  assert.ok(
    droppedRecords > 0n,
    "the bounded log queue did not report dropped records",
  );
  console.log(
    `Node logging: queue overflow reported ${droppedRecords} dropped records`,
  );
}
