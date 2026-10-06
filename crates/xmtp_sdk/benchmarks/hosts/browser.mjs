// Run one bench request in headless Chromium. Usage: node browser.mjs host.json
// Vite serves the staged browser package; the page runs it with its worker.
import { readFile, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

console.log = (...values) => console.error(...values);
const config = JSON.parse(await readFile(process.argv[2], "utf8"));
let input = "";
for await (const part of process.stdin) input += part;
const request = JSON.parse(input);
const root = request.state_directory;
const fixture = JSON.parse(await readFile(join(root, "fixture.json"), "utf8"));
const { createServer } = await import(pathToFileURL(config.vite_entry).href);
const { chromium } = await import(pathToFileURL(config.playwright_entry).href);
const hosts = dirname(fileURLToPath(import.meta.url));
const server = await createServer({
  root: hosts,
  cacheDir: join(root, "vite-cache"),
  configFile: false,
  logLevel: "error",
  resolve: {
    alias: {
      "@bench/sdk": config.sdk_entry,
      "@bench/pure": config.pure_entry,
      "@bench/accounts": config.accounts_entry,
    },
  },
  server: {
    host: "127.0.0.1",
    port: config.browser_port,
    strictPort: true,
    fs: { allow: [hosts, config.package_root, config.tools_root] },
    headers: {
      "Cross-Origin-Opener-Policy": "same-origin",
      "Cross-Origin-Embedder-Policy": "require-corp",
    },
  },
});
let context;
try {
  await server.listen();
  // One persistent profile keeps OPFS client databases between calls.
  context = await chromium.launchPersistentContext(
    join(root, "chromium-profile"),
    { headless: true },
  );
  const page = await context.newPage();
  page.on("console", (event) => console.error(event.text()));
  await page.goto(`http://127.0.0.1:${config.browser_port}/browser.html`);
  await page.waitForFunction(() => typeof window.benchmark === "function");
  const stateFile = join(
    root,
    request.workload === "stream"
      ? `stream-${request.sample}.json`
      : "page.json",
  );
  const state =
    request.phase === "measure"
      ? JSON.parse(await readFile(stateFile, "utf8"))
      : undefined;
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
