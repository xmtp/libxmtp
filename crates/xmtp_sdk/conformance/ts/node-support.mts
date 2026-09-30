import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { createServer, type Server } from "node:http";
import {
  connect,
  createServer as createNetServer,
  type AddressInfo,
  type Server as NetServer,
  type Socket,
} from "node:net";

export async function assertNoUnhandledRejection(
  action: () => Promise<void>,
): Promise<void> {
  const unhandled: unknown[] = [];
  const capture = (error: unknown): void => {
    unhandled.push(error);
  };
  process.on("unhandledRejection", capture);
  try {
    await action();
    await new Promise((resolve) => setImmediate(resolve));
    assert.deepEqual(unhandled, [], "stream left an unhandled rejection");
  } finally {
    process.off("unhandledRejection", capture);
  }
}

/** The deployment directory name of an identifier, from the identifier alone. */
export function deploymentComponent(identifier: string): string {
  // A short printable identifier with no character the file name table
  // removes keeps its bytes, lowercased.
  assert.ok(
    /^[\x20-\x7e]{1,190}$/.test(identifier) &&
      !/[<>:"|?*/\\]/.test(identifier) &&
      !/^[. ]|[. ]$/.test(identifier),
    `the expected deployment directory assumes a file-safe identifier: ${identifier}`,
  );
  const digest = createHash("sha256").update(identifier, "utf8").digest("hex");
  return `${identifier.toLowerCase()}-${digest}`;
}

async function listen(server: Server | NetServer): Promise<number> {
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  return (server.address() as AddressInfo).port;
}

/**
 * A TCP relay to the backend that counts the connections it accepts. After
 * `refuse`, it closes every connection it accepts.
 */
export async function countingRelay(backendUrl: string) {
  const target = new URL(backendUrl);
  const sockets = new Set<Socket>();
  const track = (socket: Socket): Socket => {
    sockets.add(socket);
    socket.on("close", () => sockets.delete(socket));
    socket.on("error", () => socket.destroy());
    return socket;
  };
  let forward = true;
  let connections = 0;
  const server = createNetServer((inbound) => {
    connections += 1;
    track(inbound);
    if (!forward) {
      inbound.destroy();
      return;
    }
    const outbound = track(connect(Number(target.port || 80), target.hostname));
    inbound.pipe(outbound).pipe(inbound);
    inbound.on("close", () => outbound.destroy());
    outbound.on("close", () => inbound.destroy());
  });
  const port = await listen(server);
  return {
    url: `http://127.0.0.1:${port}`,
    connections: () => connections,
    refuse(): void {
      forward = false;
      connections = 0;
    },
    async close(): Promise<void> {
      for (const socket of sockets) socket.destroy();
      await new Promise((resolve) => server.close(resolve));
    },
  };
}

/** Answer every request with one status and body, and count the requests. */
export async function serve(status: number, body: Uint8Array) {
  let requests = 0;
  const server = createServer((request, response) => {
    requests += 1;
    request.resume();
    response.writeHead(status, {
      "content-length": body.byteLength,
      connection: "close",
    });
    response.end(body);
  });
  const port = await listen(server);
  return {
    url: `http://127.0.0.1:${port}/file`,
    requests: () => requests,
    async close(): Promise<void> {
      server.closeAllConnections();
      await new Promise((resolve) => server.close(resolve));
    },
  };
}
