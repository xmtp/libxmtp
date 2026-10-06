import assert from "node:assert/strict";
import { once } from "node:events";
import { createServer } from "node:http2";
import { gunzipSync } from "node:zlib";

import {
  initLogging,
  setLogSink,
  flushTelemetry,
  localSignerFromPrivateKey,
  XmtpError,
} from "@xmtp/node-sdk";

const mode = process.argv[2];
const collectors = [];
async function collector() {
  const server = createServer();
  const sessions = new Set();
  const requests = [];
  server.on("session", (session) => {
    sessions.add(session);
    session.on("close", () => sessions.delete(session));
  });
  server.on("stream", (stream, headers) => {
    const chunks = [];
    stream.on("data", (chunk) => chunks.push(chunk));
    stream.on("end", () => {
      const frame = Buffer.concat(chunks);
      const payload =
        frame[0] === 1 ? gunzipSync(frame.subarray(5)) : frame.subarray(5);
      requests.push({ path: headers[":path"], payload });
      stream.respond(
        { ":status": 200, "content-type": "application/grpc" },
        { waitForTrailers: true },
      );
      stream.on("wantTrailers", () =>
        stream.sendTrailers({ "grpc-status": "0" }),
      );
      stream.end(Buffer.alloc(5));
    });
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const result = {
    endpoint: `http://127.0.0.1:${server.address().port}`,
    requests,
    async close() {
      for (const session of sessions) session.destroy();
      await new Promise((resolve) => server.close(resolve));
    },
  };
  collectors.push(result);
  return result;
}
async function waitFor(predicate) {
  const start = Date.now();
  while (!predicate()) {
    assert.ok(
      Date.now() - start < 5000,
      "native logging or OTLP export did not settle",
    );
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
}
try {
  const first = await collector();
  const second = await collector();
  // Before initLogging, both sink forms fail with the public error.
  for (const sink of [{ log: async () => {} }, undefined])
    await assert.rejects(
      setLogSink(sink),
      (error) => error instanceof XmtpError.InvalidInput,
    );
  const service = "node-otel-startup-service";
  const attribute = "node-otel-startup-resource";
  await initLogging({
    level: "error",
    otel:
      mode === "otel"
        ? { endpoint: first.endpoint, serviceName: service, sampleRatio: 1 }
        : undefined,
    resourceAttributes: new Map([["proof.attribute", attribute]]),
  });
  let records = 0;
  let firstRecord;
  await setLogSink({
    async log(record) {
      records += 1;
      firstRecord ??= record;
    },
  });
  await assert.rejects(localSignerFromPrivateKey(new Uint8Array(31)));
  await waitFor(() => records > 0);
  // The public sink gets the public record: a string level, not the binding
  // number.
  assert.equal(firstRecord.level, "error");
  assert.ok(firstRecord.fields instanceof Map);
  assert.equal(typeof firstRecord.droppedRecords, "bigint");
  await flushTelemetry();
  if (mode === "otel") {
    await waitFor(() =>
      first.requests.some(({ path }) => path.endsWith("LogsService/Export")),
    );
    const payload = Buffer.concat(first.requests.map(({ payload }) => payload));
    assert.ok(
      payload.includes(Buffer.from(service)),
      "service name missing from native OTLP export",
    );
    assert.ok(
      payload.includes(Buffer.from(attribute)),
      "resource attribute missing from native OTLP export",
    );
  } else {
    assert.equal(first.requests.length, 0);
  }
  const before = first.requests.length;
  const priorRecords = records;
  await initLogging({
    level: "debug",
    otel: {
      endpoint: second.endpoint,
      serviceName: "ignored-second-service",
      sampleRatio: 1,
    },
  });
  await assert.rejects(localSignerFromPrivateKey(new Uint8Array(31)));
  await waitFor(() => records > priorRecords);
  await flushTelemetry();
  if (mode === "otel") await waitFor(() => first.requests.length > before);
  assert.equal(
    second.requests.length,
    0,
    "repeat init replaced the first logging exporter",
  );
  // A cleared sink gets no later record. Records already handed to the JS
  // thread may still land, so settle before the count is taken.
  await setLogSink(undefined);
  await new Promise((resolve) => setTimeout(resolve, 100));
  const recordsAtClear = records;
  await assert.rejects(localSignerFromPrivateKey(new Uint8Array(31)));
  await new Promise((resolve) => setTimeout(resolve, 500));
  assert.equal(records, recordsAtClear, "a cleared log sink was called");
  console.log(
    JSON.stringify({
      result: "PASS",
      mode,
      records,
      exports: first.requests.length,
    }),
  );
} finally {
  for (const collector of collectors) await collector.close();
}
