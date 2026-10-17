import { createReadStream } from "node:fs";
import { stat } from "node:fs/promises";
import { createServer } from "node:http";
import { extname, join, resolve } from "node:path";

const TYPES = {
  ".css": "text/css",
  ".html": "text/html",
  ".js": "text/javascript",
  ".json": "application/json",
  ".svg": "image/svg+xml",
  ".txt": "text/plain",
};

export function serveStatic({ root, host = "127.0.0.1", port = 4322 }) {
  const absoluteRoot = resolve(root);
  const server = createServer(async (request, response) => {
    let pathname;
    try {
      pathname = decodeURIComponent(
        new URL(request.url, `http://${host}`).pathname,
      );
    } catch {
      response.writeHead(400).end("Invalid URL");
      return;
    }
    let path = resolve(absoluteRoot, `.${pathname}`);
    if (path !== absoluteRoot && !path.startsWith(`${absoluteRoot}/`)) {
      response.writeHead(403).end();
      return;
    }
    try {
      if ((await stat(path)).isDirectory()) path = join(path, "index.html");
      await stat(path);
      response.setHeader(
        "content-type",
        TYPES[extname(path)] ?? "application/octet-stream",
      );
      createReadStream(path).pipe(response);
    } catch {
      response.writeHead(404).end("Not found");
    }
  });
  return new Promise((accept, reject) => {
    server.once("error", reject);
    server.listen(port, host, () => accept(server));
  });
}

if (import.meta.url === new URL(`file://${process.argv[1]}`).href) {
  await serveStatic({
    root: process.env.SITE_ROOT ?? resolve(import.meta.dirname, "../_site"),
    host: process.env.DOCS_HOST ?? "127.0.0.1",
    port: Number(process.env.DOCS_PORT ?? 4322),
  });
  console.log(
    `Documentation server listening on http://127.0.0.1:${process.env.DOCS_PORT ?? 4322}`,
  );
}
