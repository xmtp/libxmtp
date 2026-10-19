import assert from "node:assert/strict";
import { createServer } from "node:http";
import test from "node:test";

import { runtimeControl, runtimeResponseSettlement } from "./peer-runtime.mjs";

function deferred() {
  let resolve;
  const promise = new Promise((settle) => {
    resolve = settle;
  });
  return { promise, resolve };
}

async function fixture(context) {
  const signer = deferred();
  const control = runtimeControl({}, async () => {
    await signer.promise;
    return { bytes: [1, 2, 3] };
  });
  const server = createServer(async (request, response) => {
    try {
      if (request.url === "/runtime/result") {
        control.assertObservation();
        response.end(JSON.stringify({ accepted: true }));
        return;
      }
      const output = await control(
        request.url,
        {},
        request.url === "/runtime/hold-sign"
          ? runtimeResponseSettlement(response)
          : undefined,
      );
      response.end(JSON.stringify(output));
    } catch (error) {
      response.writeHead(500).end(JSON.stringify({ error: String(error) }));
    }
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  context.after(async () => {
    signer.resolve();
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
  });
  const call = async (path, options, expectedStatus = 200) => {
    const response = await fetch(
      `http://127.0.0.1:${server.address().port}/runtime/${path}`,
      options,
    );
    assert.equal(response.status, expectedStatus);
    return response.json();
  };
  return { call, signer };
}

test(
  "reload observation waits for the released signer and a cancelled HTTP response",
  { timeout: 10_000 },
  async (context) => {
    const { call, signer } = await fixture(context);
    const abort = new AbortController();
    const held = call("hold-sign", { signal: abort.signal }).catch(
      (error) => error,
    );
    await call("prepare");
    abort.abort();
    await held;
    await call("resume");
    let complete = false;
    const observing = call("observe").then((result) => {
      complete = true;
      return result;
    });
    void observing.catch(() => {});
    const pending = await call("status");
    assert.equal(pending.signOutcome, "pending");
    await call("result", undefined, 500);
    assert.equal(
      complete,
      false,
      "unsettled signer must not count as safe reload",
    );
    signer.resolve();
    const settled = await observing;
    assert.equal(settled.signOutcome, "fulfilled");
    assert.equal(settled.responseOutcome, "closed");
    assert.equal(settled.observationMs, 2000);
    assert.equal(settled.staleCalls, 0);
    assert.deepEqual(await call("result"), { accepted: true });
    await call("stale");
    await call("result", undefined, 500);
  },
);

test(
  "reload observation catches a stale callback delayed until after the signer response",
  { timeout: 10_000 },
  async (context) => {
    const { call, signer } = await fixture(context);
    const held = call("hold-sign");
    void held.catch(() => {});
    await call("prepare");
    await call("resume");
    await call("result", undefined, 500);
    signer.resolve();
    await held;
    const observation = call("observe");
    void observation.catch(() => {});
    // Model an incorrectly retained old-runtime continuation: its HTTP response
    // has finished, but its application callback is queued for a later turn.
    await new Promise((resolve) => setTimeout(resolve, 100));
    await call("stale");
    const settled = await observation;
    assert.equal(settled.signOutcome, "fulfilled");
    assert.equal(settled.responseOutcome, "finished");
    assert.equal(
      settled.staleCalls,
      1,
      "post-response stale callback must fail the installed reload gate",
    );
    await call("result", undefined, 500);
  },
);
