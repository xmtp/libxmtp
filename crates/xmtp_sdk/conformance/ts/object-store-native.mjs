// Native tonic clients use HTTP/2. This relay preserves their gRPC trailers.
import { connect, createServer } from "node:http2";

export async function nativeRelay(responseTransform) {
  const target = new URL(process.env.SDK_RELAY_TARGET);
  const session = connect(target.origin);
  session.on("error", (error) => console.error("native relay session", error));
  const server = createServer();
  server.on("stream", (incoming, headers) => {
    const transform = responseTransform(headers[":path"]);
    const outgoing = session.request({
      ...headers,
      ":authority": target.host,
      ":scheme": target.protocol.slice(0, -1),
    });
    let trailers = {};
    outgoing.on("trailers", (value) => {
      trailers = value;
    });
    incoming.on("wantTrailers", () => incoming.sendTrailers(trailers));
    outgoing.on("response", (response) => {
      if (!transform || response[":status"] !== 200) {
        incoming.respond(response, { waitForTrailers: true });
        outgoing.pipe(incoming);
        return;
      }
      const chunks = [];
      outgoing.on("data", (chunk) => chunks.push(chunk));
      outgoing.on("end", () => {
        if (incoming.destroyed) return;
        try {
          const body = transform(Buffer.concat(chunks));
          delete response["content-length"];
          incoming.respond(response, { waitForTrailers: true });
          incoming.end(body);
        } catch (error) {
          console.error("native relay response", error);
          incoming.respond({ ":status": 502 });
          incoming.end();
        }
      });
    });
    outgoing.on("error", (error) => {
      console.error("native relay request", error);
      if (!incoming.destroyed) {
        if (!incoming.headersSent) incoming.respond({ ":status": 502 });
        incoming.end();
      }
    });
    incoming.on("error", () => outgoing.close());
    incoming.on("close", () => outgoing.close());
    incoming.pipe(outgoing);
  });
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  return `http://127.0.0.1:${server.address().port}`;
}
