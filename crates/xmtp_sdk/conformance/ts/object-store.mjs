// A loopback object store for SDK conformance. PUT stores a body at its path,
// GET returns it, and /status/<code> answers with that status. It checks no
// signature, so it stands in for S3 only on a test machine. It prints its URL
// once it listens.
import { createServer } from "node:http";

const objects = new Map();
const server = createServer(async (request, response) => {
  const { pathname } = new URL(request.url ?? "/", "http://127.0.0.1");
  const chunks = [];
  for await (const chunk of request) {
    chunks.push(chunk);
  }
  const status = /^\/status\/(\d{3})$/.exec(pathname);
  if (status) {
    response.writeHead(Number(status[1])).end();
  } else if (request.method === "PUT") {
    if (request.headers["if-none-match"] === "*" && objects.has(pathname)) {
      response.writeHead(412).end();
      return;
    }
    objects.set(pathname, Buffer.concat(chunks));
    response.writeHead(200).end();
  } else if (
    (request.method === "GET" || request.method === "HEAD") &&
    objects.has(pathname)
  ) {
    const body = objects.get(pathname);
    response.writeHead(200, { "content-length": body.length }).end(body);
  } else {
    response.writeHead(404).end();
  }
});
server.listen(Number(process.argv[2] ?? 0), "127.0.0.1", () => {
  console.log(`http://127.0.0.1:${server.address().port}`);
});
