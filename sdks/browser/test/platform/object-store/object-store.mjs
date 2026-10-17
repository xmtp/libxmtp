// A loopback object store for the browser platform proofs. PUT stores a body at its path,
// GET returns it, /status/<code> answers with that status, /redirect answers
// with a redirect, and /hang never answers. It checks no signature, so it
// stands in for S3 only on a test machine. Every response allows any origin,
// so a browser worker can read it. It prints its URL once it listens.
//
// /relay/<id>/<path> forwards to SDK_RELAY_TARGET until a request to
// /refuse/<id>. After that the relay answers 503 and /count/<id> reports how
// many requests other than preflights it refused.
import { createServer, request as httpRequest } from "node:http";
import { request as httpsRequest } from "node:https";

import { transferRoute } from "./object-store-transfer.mjs";

const objects = new Map();
const refused = new Map();

function relay(request, response, path, body, transform) {
  const target = new URL(path, process.env.SDK_RELAY_TARGET);
  const forward = target.protocol === "https:" ? httpsRequest : httpRequest;
  const upstream = forward(
    target,
    {
      method: request.method,
      headers: { ...request.headers, host: target.host },
    },
    (reply) => {
      if (!transform || reply.statusCode !== 200) {
        response.writeHead(reply.statusCode ?? 502, reply.headers);
        reply.pipe(response);
        return;
      }
      const chunks = [];
      reply.on("data", (chunk) => chunks.push(chunk));
      reply.on("end", () => {
        try {
          const body = transform(Buffer.concat(chunks));
          const headers = { ...reply.headers };
          delete headers["content-length"];
          response.writeHead(200, headers).end(body);
        } catch (error) {
          console.error(error);
          response.writeHead(502).end();
        }
      });
    },
  );
  upstream.on("error", () => {
    if (!response.headersSent) response.writeHead(502);
    response.end();
  });
  upstream.end(body);
}

const server = createServer(async (request, response) => {
  const { pathname, search } = new URL(request.url ?? "/", "http://127.0.0.1");
  const chunks = [];
  for await (const chunk of request) {
    chunks.push(chunk);
  }
  response.setHeader("access-control-allow-origin", "*");
  const relayed = /^\/relay\/([^/]+)(\/.*)?$/.exec(pathname);
  const refusal = /^\/(refuse|count)\/([^/]+)$/.exec(pathname);
  const status = /^\/status\/(\d{3})$/.exec(pathname);
  if (relayed && !refused.has(relayed[1])) {
    relay(
      request,
      response,
      `${relayed[2] ?? "/"}${search}`,
      Buffer.concat(chunks),
    );
  } else if (request.method === "OPTIONS") {
    response
      .writeHead(204, {
        "access-control-allow-methods": "GET, HEAD, POST, PUT",
        "access-control-allow-headers": "*",
      })
      .end();
  } else if (
    transferRoute(request, response, pathname, Buffer.concat(chunks), relay)
  ) {
    // The lifetime fixture holds PUT responses until its release request.
  } else if (relayed) {
    refused.set(relayed[1], refused.get(relayed[1]) + 1);
    response.writeHead(503).end();
  } else if (refusal?.[1] === "refuse") {
    refused.set(refusal[2], refused.get(refusal[2]) ?? 0);
    response.writeHead(200).end();
  } else if (refusal) {
    response.writeHead(200).end(String(refused.get(refusal[2]) ?? 0));
  } else if (status) {
    response.writeHead(Number(status[1])).end();
  } else if (pathname === "/hang") {
    // Leave the request open until the client goes away.
  } else if (pathname === "/redirect") {
    response.writeHead(302, { location: "/status/200" }).end();
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
