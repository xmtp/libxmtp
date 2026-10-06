import { once } from "node:events";
import {
  createServer as createHttpServer,
  type ServerResponse,
} from "node:http";
import {
  connect,
  createServer as createHttp2Server,
  type Http2Session,
} from "node:http2";
import { isAbsolute, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import protobuf from "protobufjs";

const UPLOAD_PATH = "/xmtp.backend.v1.AttachmentService/CreateUpload";

/** Change the URL in each gRPC message frame of a CreateUpload response. */
function redirectGrant(
  body: Buffer,
  url: string,
  grant: protobuf.Type,
): Buffer {
  const frames: Buffer[] = [];
  for (let offset = 0; offset < body.length;) {
    const flag = body[offset]!;
    const size = body.readUInt32BE(offset + 1);
    let payload = body.subarray(offset + 5, offset + 5 + size);
    if (flag === 0)
      payload = Buffer.from(
        grant
          .encode({ ...grant.toObject(grant.decode(payload)), url })
          .finish(),
      );
    const header = Buffer.alloc(5);
    header[0] = flag;
    header.writeUInt32BE(payload.length, 1);
    frames.push(header, payload);
    offset += 5 + size;
  }
  return Buffer.concat(frames);
}

/**
 * A relay to the real backend whose upload grants point at a local store.
 * The store holds each PUT response until `release()`. Every other call
 * reaches the backend unchanged. Native clients use HTTP/2, so the relay
 * keeps their gRPC trailers.
 */
export async function heldUploadBackend(
  backendUrl = process.env.XMTP_BACKEND_URL!,
) {
  const protoRoot = fileURLToPath(new URL("../../../proto/", import.meta.url));
  const root = new protobuf.Root();
  root.resolvePath = (_, target) =>
    isAbsolute(target) ? target : resolve(protoRoot, target);
  await root.load("backend/v1/backend.proto");
  const grant = root.lookupType("xmtp.backend.v1.CreateUploadResponse");

  let puts = 0;
  let grants = 0;
  let released = false;
  const held: ServerResponse[] = [];
  const entered = Promise.withResolvers<void>();
  const store = createHttpServer((request, response) => {
    request.resume();
    request.on("end", () => {
      puts += 1;
      entered.resolve();
      if (released) response.writeHead(200).end();
      else held.push(response);
    });
  });
  store.listen(0, "127.0.0.1");
  await once(store, "listening");
  const storeAddress = store.address();
  if (!storeAddress || typeof storeAddress === "string")
    throw new Error("the held store has no TCP address");
  const putUrl = `http://127.0.0.1:${storeAddress.port}/put`;

  const target = new URL(backendUrl);
  const upstream = connect(target.origin);
  upstream.on("error", () => {});
  const sessions = new Set<Http2Session>();
  const relay = createHttp2Server();
  relay.on("session", (session) => {
    sessions.add(session);
    session.on("close", () => sessions.delete(session));
  });
  relay.on("stream", (incoming, headers) => {
    const redirect = headers[":path"] === UPLOAD_PATH;
    if (redirect) grants += 1;
    const outgoing = upstream.request({
      ...headers,
      ":authority": target.host,
      ":scheme": target.protocol.slice(0, -1),
    });
    let trailers = {};
    outgoing.on("trailers", (value) => (trailers = value));
    incoming.on("wantTrailers", () => incoming.sendTrailers(trailers));
    outgoing.on("response", (response) => {
      if (!redirect || response[":status"] !== 200) {
        incoming.respond(response, { waitForTrailers: true });
        outgoing.pipe(incoming);
        return;
      }
      const chunks: Buffer[] = [];
      outgoing.on("data", (chunk: Buffer) => chunks.push(chunk));
      outgoing.on("end", () => {
        if (incoming.destroyed) return;
        delete response["content-length"];
        incoming.respond(response, { waitForTrailers: true });
        incoming.end(redirectGrant(Buffer.concat(chunks), putUrl, grant));
      });
    });
    outgoing.on("error", () => {
      if (incoming.destroyed) return;
      if (!incoming.headersSent) incoming.respond({ ":status": 502 });
      incoming.end();
    });
    incoming.on("error", () => outgoing.close());
    incoming.on("close", () => outgoing.close());
    incoming.pipe(outgoing);
  });
  relay.listen(0, "127.0.0.1");
  await once(relay, "listening");
  const relayAddress = relay.address();
  if (!relayAddress || typeof relayAddress === "string")
    throw new Error("the upload relay has no TCP address");

  const release = () => {
    released = true;
    for (const response of held.splice(0)) response.writeHead(200).end();
  };
  return {
    url: `http://127.0.0.1:${relayAddress.port}`,
    /** Resolves when the first PUT reaches the store. */
    entered: entered.promise,
    counts: () => ({ puts, grants }),
    release,
    async close() {
      release();
      for (const session of sessions) session.destroy();
      upstream.destroy();
      store.closeAllConnections();
      await Promise.all(
        [relay, store].map(
          (server) =>
            new Promise<void>((done, reject) =>
              server.close((error) => (error ? reject(error) : done())),
            ),
        ),
      );
    },
  };
}
