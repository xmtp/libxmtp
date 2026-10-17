// Controlled upload responses for the attachment lifetime proofs. All backend
// calls still reach the real backend. Only the granted target URL changes.
import { spawnSync } from "node:child_process";

const transfers = new Map();
const uploadPath = "/xmtp.backend.v1.AttachmentService/CreateUpload";

function protobuf(mode, input) {
  const result = spawnSync(
    process.env.SDK_PROTOC ?? "protoc",
    [
      "-Iproto",
      `--${mode}=xmtp.backend.v1.CreateUploadResponse`,
      "proto/backend/v1/backend.proto",
    ],
    { input },
  );
  if (result.status !== 0) throw new Error(String(result.stderr));
  return result.stdout;
}

// Keep the grant's method, headers and expiry, and all gRPC-web trailers.
// The fixture does not check signatures. Normal transfer tests own that proof.
function redirectGrant(body, url) {
  const frames = [];
  for (let offset = 0; offset < body.length;) {
    const flag = body[offset];
    const size = body.readUInt32BE(offset + 1);
    let payload = body.subarray(offset + 5, offset + 5 + size);
    if (flag === 0) {
      const grant = protobuf("decode", payload).toString();
      if (!/^url: /m.test(grant)) throw new Error("upload grant has no URL");
      payload = protobuf(
        "encode",
        grant.replace(/^url: .*$/m, `url: ${JSON.stringify(url)}`),
      );
    } else if (flag !== 128) {
      throw new Error(`unexpected upload response frame ${flag}`);
    }
    const header = Buffer.alloc(5);
    header[0] = flag;
    header.writeUInt32BE(payload.length, 1);
    frames.push(header, payload);
    offset += size + 5;
  }
  return Buffer.concat(frames);
}

export function transferRoute(request, response, pathname, body, relay) {
  const match = /^\/transfer\/([^/]+)\/(.*)$/.exec(pathname);
  if (!match) return false;
  const [, id, action] = match;
  if (action === "arm") {
    if (transfers.has(id)) throw new Error("transfer already armed");
    transfers.set(id, {
      puts: 0,
      grants: 0,
      gets: 0,
      entered: [],
      downloads: [],
      responses: [],
      released: false,
    });
    response.writeHead(200).end();
    return true;
  }
  const transfer = transfers.get(id);
  if (!transfer) {
    response.writeHead(404).end("transfer is not armed");
    return true;
  }
  if (action.startsWith("backend/")) {
    const path = `/${action.slice("backend/".length)}`;
    if (path === uploadPath) transfer.grants++;
    const transform =
      path === uploadPath
        ? (reply) => {
            return redirectGrant(
              reply,
              `http://${request.headers.host}/transfer/${id}/put`,
            );
          }
        : undefined;
    relay(request, response, path, body, transform);
  } else if (action === "put" && request.method === "PUT") {
    transfer.puts++;
    transfer.body = body;
    for (const waiter of transfer.entered.splice(0))
      waiter.writeHead(200).end();
    if (transfer.released) response.writeHead(200).end();
    else transfer.responses.push(response);
  } else if (action === "download") {
    transfer.gets++;
    for (const waiter of transfer.downloads.splice(0))
      waiter.writeHead(200).end();
    transfer.responses.push(response);
  } else if (action === "download-entered") {
    if (transfer.gets > 0) response.writeHead(200).end();
    else transfer.downloads.push(response);
  } else if (action === "entered") {
    if (transfer.puts > 0) response.writeHead(200).end();
    else transfer.entered.push(response);
  } else if (action === "release") {
    transfer.released = true;
    for (const held of transfer.responses.splice(0)) held.writeHead(200).end();
    response.writeHead(200).end();
  } else if (action === "counts") {
    response.writeHead(200, { "content-type": "application/json" }).end(
      JSON.stringify({
        puts: transfer.puts,
        grants: transfer.grants,
        gets: transfer.gets,
      }),
    );
  } else response.writeHead(404).end();
  return true;
}
