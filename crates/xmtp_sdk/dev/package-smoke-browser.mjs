#!/usr/bin/env node
import assert from "node:assert/strict";
import { readFileSync, existsSync } from "node:fs";
import { createServer } from "node:http";
import { resolve, join } from "node:path";

import { chromium } from "../../../sdks/browser/node_modules/playwright/index.mjs";
import { packageAsset } from "./package-smoke-path.mjs";
const packageRoot = resolve(process.argv[2]);
const runtimeEntry = (name) => {
  const manifest = JSON.parse(
    readFileSync(join(packageRoot, "node_modules/@ubjs", name, "package.json")),
  );
  const entry =
    manifest.module ?? manifest.exports?.["."]?.browser ?? manifest.main;
  assert.equal(typeof entry, "string");
  return `/node_modules/@ubjs/${name}/${entry.replace(/^\.\//, "")}`;
};
const html = `<!doctype html><script type="importmap">${JSON.stringify({
  imports: {
    "@ubjs/core": runtimeEntry("core"),
    "@ubjs/wasm": runtimeEntry("wasm"),
  },
  scopes: {
    "/typescript-wasm/": { "#xmtp/binding": "/typescript-wasm/xmtp_sdk.js" },
    "/typescript-pure/": { "#xmtp/binding": "/typescript-pure/xmtp_sdk.js" },
  },
})}</script>`;
const requests = [];
const server = createServer((request, response) => {
  const path = new URL(request.url, "http://localhost").pathname;
  requests.push(path);
  if (path === "/") {
    response.setHeader("Content-Type", "text/html");
    response.end(html);
    return;
  }
  const file = packageAsset(packageRoot, path);
  if (file === undefined || !existsSync(file)) {
    response.writeHead(404);
    response.end();
    return;
  }
  response.setHeader(
    "Content-Type",
    file.endsWith(".wasm")
      ? "application/wasm"
      : file.endsWith(".json")
        ? "application/json"
        : "text/javascript",
  );
  response.end(readFileSync(file));
});
await new Promise((done) => server.listen(0, "127.0.0.1", done));
const browser = await chromium.launch({ headless: true });
try {
  const url = `http://127.0.0.1:${server.address().port}`;
  const load = async () => {
    const page = await browser.newPage();
    await page.goto(url);
    try {
      return await page.evaluate(async () => {
        const sdk = await import("/entry.js");
        const pure = await import("/pure.js");
        await pure.initPureWasm();
        const codec = new pure.TextCodec();
        if (codec.decode(codec.encode("installed")) !== "installed")
          throw new Error("installed codec round trip failed");
        if (typeof sdk.Client !== "function")
          throw new Error("installed worker root has no Client");
        const worker = new Worker(
          new URL("/typescript-wasm/worker-entry.gen.js", location.href),
          { type: "module" },
        );
        // The worker loads the installed WASM package, including its snippets.
        const result = await new Promise((done, fail) => {
          worker.onerror = (event) => fail(new Error(event.message));
          worker.onmessage = (event) =>
            event.data.t === "ready"
              ? done(event.data)
              : event.data.t === "fatal" || event.data.t === "refused"
                ? fail(new Error(JSON.stringify(event.data)))
                : undefined;
          void import("/typescript-wasm/contract.gen.js").then(
            ({ PROTOCOL_VERSION, CONTRACT_HASH }) => {
              worker.postMessage({
                t: "hello",
                version: PROTOCOL_VERSION,
                hash: CONTRACT_HASH,
                lifetimeLock: crypto.randomUUID(),
              });
            },
          );
          setTimeout(
            () => fail(new Error("installed worker startup timed out")),
            20000,
          );
        });
        worker.terminate();
        return result;
      });
    } finally {
      await page.close();
    }
  };
  await load();
  assert.ok(!requests.some((path) => path.endsWith("sdk-contract.json")));
  assert.ok(!requests.some((path) => path.endsWith("contract-check.js")));
  console.log(
    "Installed browser pure and worker load passed without package receipt requests",
  );
} finally {
  await browser.close();
  await new Promise((done) => server.close(done));
}
