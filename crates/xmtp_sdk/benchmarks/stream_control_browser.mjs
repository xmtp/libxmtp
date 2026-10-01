import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
const tools = resolve(process.argv[3]);
const { createServer } = await import(
  pathToFileURL(resolve(tools, "vite/dist/node/index.js")).href
);
const { chromium } = await import(
  pathToFileURL(resolve(tools, "playwright/index.mjs")).href
);
const fixture = JSON.parse(await readFile(process.argv[2], "utf8"));
const root = dirname(fileURLToPath(import.meta.url));
const mutation = process.argv[4] ? resolve(process.argv[4]) : undefined;
const server = await createServer({
  root,
  configFile: false,
  logLevel: "error",
  cacheDir: resolve(root, "../../../target/benchmark-stream-vite"),
  server: {
    host: "127.0.0.1",
    port: 0,
    fs: { allow: [root, ...(mutation ? [dirname(mutation)] : [])] },
  },
  plugins: [
    {
      name: "stream-control-page",
      configureServer(server) {
        server.middlewares.use((request, response, next) => {
          if (request.url !== "/") return next();
          response.setHeader("Content-Type", "text/html");
          response.end(
            '<script type="module">import { runStreamControls } from "/stream_control.mjs"; window.runStreamControls = runStreamControls;</script>',
          );
        });
      },
    },
  ],
});
let browser;
try {
  await server.listen();
  browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`);
  await page.waitForFunction(
    () => typeof window.runStreamControls === "function",
  );
  const result = await page.evaluate(
    async ([fixture, mutation]) => {
      const measured = mutation
        ? (await import(/* @vite-ignore */ `/@fs/${mutation}`)).measure
        : undefined;
      return window.runStreamControls(fixture, "browser", measured);
    },
    [fixture, mutation],
  );
  console.log(JSON.stringify(result, null, 2));
} finally {
  await browser?.close();
  await server.close();
}
