import { createReadStream } from "node:fs";
import { join } from "node:path";

import { createServer } from "../../../node_modules/vite/dist/node/index.js";

export async function createMigrationServer(directory, names) {
  const server = await createServer({
    plugins: [
      {
        name: "migration-row-fixtures",
        configureServer(server) {
          server.middlewares.use((request, response, next) => {
            const name = request.url?.replace(/^\/row-fixture\//, "");
            if (!names.includes(name)) return next();
            response.setHeader("Content-Type", "application/octet-stream");
            createReadStream(join(directory, name + ".db3")).pipe(response);
          });
        },
      },
    ],
    root: process.cwd(),
    configFile: false,
    cacheDir: "target/migration-message-size-vite",
    optimizeDeps: { noDiscovery: true },
    server: {
      hmr: false,
      watch: null,
      host: "127.0.0.1",
      port: 0,
      fs: { strict: false },
      proxy: {
        "/backend": {
          target: process.env.XMTP_BACKEND_URL ?? "http://127.0.0.1:5050",
          changeOrigin: true,
          rewrite: (path) => path.replace(/^\/backend/, ""),
        },
      },
    },
  });
  await server.listen();
  return server;
}

export async function withMigrationPage(browser, server, run) {
  const context = await browser.newContext();
  try {
    const page = await context.newPage();
    await page.goto(
      "http://127.0.0.1:" +
        server.httpServer.address().port +
        "/sdks/browser/test/platform/migration/browser.html",
    );
    return await run(page);
  } finally {
    await context.close();
  }
}
