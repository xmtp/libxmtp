import { createServer, createConnection, type Socket } from "node:net";

type Fault =
  | "disconnect"
  | "drain"
  | "flap"
  | "blackhole-inbound"
  | "blackhole-outbound";

// Each test owns its proxy. It cannot interrupt other clients on the backend.
export async function createRecoveryProxy(
  backendUrl = process.env.XMTP_BACKEND_URL!,
) {
  const backend = new URL(backendUrl);
  const sockets = new Set<Socket>();
  const flapTimers = new Map<Socket, ReturnType<typeof setTimeout>>();
  let flapDrops = 0;
  let fault: "none" | Fault = "none";
  const cancelFlapTimers = () => {
    for (const timer of flapTimers.values()) clearTimeout(timer);
    flapTimers.clear();
  };
  const failSoon = (socket: Socket) => {
    if (flapTimers.has(socket)) return;
    const timer = setTimeout(() => {
      flapTimers.delete(socket);
      if (fault === "flap" && !socket.destroyed) {
        flapDrops += 1;
        socket.destroy();
      }
    }, 100);
    flapTimers.set(socket, timer);
  };
  const server = createServer((downstream) => {
    if (fault === "disconnect" || fault === "drain") {
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
        const timer = flapTimers.get(socket);
        if (timer !== undefined) clearTimeout(timer);
        flapTimers.delete(socket);
        downstream.destroy();
        upstream.destroy();
      });
    }
    downstream.on("data", (data: Buffer) => {
      if (
        (fault === "none" ||
          fault === "flap" ||
          fault === "blackhole-inbound") &&
        !upstream.write(data)
      )
        downstream.pause();
    });
    upstream.on("drain", () => downstream.resume());
    upstream.on("data", (data: Buffer) => {
      if (
        (fault === "none" ||
          fault === "flap" ||
          fault === "blackhole-outbound") &&
        !downstream.write(data)
      )
        upstream.pause();
    });
    downstream.on("drain", () => upstream.resume());
    if (fault === "flap") failSoon(downstream);
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
    get flapDrops() {
      return flapDrops;
    },
    disconnect() {
      cancelFlapTimers();
      fault = "disconnect";
      for (const socket of sockets) socket.destroy();
    },
    inject(next: Fault) {
      cancelFlapTimers();
      fault = next;
      if (next === "disconnect") {
        for (const socket of sockets) socket.destroy();
      } else if (next === "drain") {
        // TCP EOF models a closed deployment connection. This does not send
        // HTTP/2 GOAWAY or restart the shared backend process.
        for (const socket of sockets) socket.end();
      } else if (next === "flap") {
        // Real backend handshakes may complete, but no connection can reach
        // Core's 30-second healthy interval. Do not change any SDK retry limit.
        for (const socket of sockets) failSoon(socket);
      }
    },
    restore() {
      fault = "none";
      cancelFlapTimers();
    },
    async close() {
      cancelFlapTimers();
      for (const socket of sockets) socket.destroy();
      await new Promise<void>((resolve, reject) => {
        server.close((error) => (error ? reject(error) : resolve()));
      });
    },
  };
}
