import { createConnection, createServer, type Socket } from "node:net";

// Each test owns its proxy and sockets. The shared backend stays available.
export async function createRecoveryProxy(
  backendUrl = process.env.XMTP_BACKEND_URL!,
) {
  const backend = new URL(backendUrl);
  const sockets = new Set<Socket>();
  let disconnected = false;
  const server = createServer((downstream) => {
    if (disconnected) {
      downstream.destroy();
      return;
    }
    const upstream = createConnection({
      host: backend.hostname,
      port: Number(backend.port),
    });
    for (const socket of [downstream, upstream]) {
      sockets.add(socket);
      socket.on("error", () => {
        downstream.destroy();
        upstream.destroy();
      });
      socket.on("close", () => {
        sockets.delete(socket);
        downstream.destroy();
        upstream.destroy();
      });
    }
    downstream.pipe(upstream);
    upstream.pipe(downstream);
  });
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  if (address === null || typeof address === "string") {
    throw new Error("The test proxy has no TCP address");
  }
  return {
    url: `http://127.0.0.1:${address.port}`,
    disconnect() {
      disconnected = true;
      for (const socket of sockets) socket.destroy();
    },
    restore() {
      disconnected = false;
    },
    async close() {
      for (const socket of sockets) socket.destroy();
      await new Promise<void>((resolve, reject) => {
        server.close((error) => (error ? reject(error) : resolve()));
      });
    },
  };
}
