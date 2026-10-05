import { readFile, writeFile, mkdir } from "node:fs/promises";
import { resolve, join, dirname } from "node:path";
import { pathToFileURL, fileURLToPath } from "node:url";

import { admitEntries } from "./entries.mjs";

console.log = (...values) => console.error(...values);
const config = JSON.parse(await readFile(process.argv[2], "utf8"));
let input = "";
for await (const part of process.stdin) input += part;
const request = JSON.parse(input);
const root = request.state_directory;
await admitEntries(config, request, "browser");

await mkdir(root, { recursive: true });
const fixture = JSON.parse(
  await readFile(request.fixture ?? join(root, "fixture.json"), "utf8"),
);
const { createServer } = await import(
  pathToFileURL(resolve(config.vite_entry)).href
);
const { chromium } = await import(
  pathToFileURL(resolve(config.playwright_entry)).href
);
const hosts = dirname(fileURLToPath(import.meta.url));
const server = await createServer({
  root: hosts,
  cacheDir: join(root, "vite-cache"),
  configFile: false,
  logLevel: "error",
  resolve: {
    alias: {
      "@bench/sdk": resolve(config.sdk_entry),
      "@bench/pure": resolve(config.pure_entry ?? config.sdk_entry),
      "@bench/accounts": resolve(config.accounts_entry),
    },
  },
  server: {
    host: "127.0.0.1",
    port: config.browser_port,
    strictPort: true,
    fs: {
      allow: [hosts, resolve(config.package_root), resolve(config.tools_root)],
    },
    headers: {
      "Cross-Origin-Opener-Policy": "same-origin",
      "Cross-Origin-Embedder-Policy": "require-corp",
    },
  },
});
let context;
try {
  await server.listen();
  context = await chromium.launchPersistentContext(
    join(root, "chromium-profile"),
    {
      headless: true,
      executablePath: config.chromium_executable,
    },
  );
  const page = await context.newPage();
  page.on("console", (event) => console.error(event.text()));
  await page.goto(`http://127.0.0.1:${config.browser_port}/browser.html`);
  await page.waitForFunction(() => typeof window.benchmark === "function");
  const stateFile = join(
    root,
    request.workload === "stream" ? `stream-${request.pair}.json` : "page.json",
  );
  let state;
  if (request.phase === "measure")
    state = JSON.parse(await readFile(stateFile, "utf8"));
  const response = await page.evaluate(
    ([req, data, saved, backend]) =>
      window.benchmark(req, data, saved, backend),
    [request, fixture, state, config.backend_url],
  );
  if (response.state) {
    await writeFile(stateFile, JSON.stringify(response.state));
    delete response.state;
  }
  process.stdout.write(JSON.stringify(response) + "\n");
} finally {
  if (context) await context.close();
  await server.close();
}
