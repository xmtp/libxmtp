#!/usr/bin/env node
import assert from "node:assert/strict";
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { createServer } from "node:http";
import { resolve, join } from "node:path";

import { chromium } from "../../../sdks/browser/node_modules/playwright/index.mjs";
const packageRoot = resolve(process.argv[2]);
const metadataFile = join(packageRoot, "sdk-contract.json");
const original = readFileSync(metadataFile, "utf8");
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
const server = createServer((request, response) => {
  const path = new URL(request.url, "http://localhost").pathname;
  if (path === "/") {
    response.setHeader("Content-Type", "text/html");
    response.end(html);
    return;
  }
  const file = resolve(packageRoot, `.${decodeURIComponent(path)}`);
  if (!file.startsWith(`${packageRoot}/`) || !existsSync(file)) {
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
  const metadata = JSON.parse(original);
  metadata.contract = "deliberate-mismatch";
  writeFileSync(metadataFile, JSON.stringify(metadata));
  await assert.rejects(load, /SDK contract mismatch/);
  writeFileSync(metadataFile, original);
  await load();
  const wasmFile = join(packageRoot, "typescript-pure/xmtp_sdk.wasm");
  const wasm = readFileSync(wasmFile);
  try {
    const wrong = Buffer.from(wasm);
    wrong[wrong.length - 1] ^= 1;
    writeFileSync(wasmFile, wrong);
    await assert.rejects(load, /SDK asset mismatch/);
  } finally {
    writeFileSync(wasmFile, wasm);
  }
  console.log(
    "Installed browser pure/worker load and contract/asset mismatch passed",
  );
} finally {
  writeFileSync(metadataFile, original);
  await browser.close();
  await new Promise((done) => server.close(done));
}
