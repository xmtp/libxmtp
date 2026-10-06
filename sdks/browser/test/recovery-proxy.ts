// Vitest browser commands run this TCP proxy in the Node process. A test
// gives a client the proxy URL, then drops and refuses its connections.
import {
  createConnection,
  createServer,
  type Server,
  type Socket,
} from "node:net";

type Proxy = { server: Server; sockets: Set<Socket>; refusing: boolean };
const proxies = new Map<string, Proxy>();

function proxy(id: string): Proxy {
  const found = proxies.get(id);
  if (!found) throw new Error(`No recovery proxy ${id}`);
  return found;
}

async function startRecoveryProxy(): Promise<{ id: string; url: string }> {
  const backend = new URL(process.env.XMTP_BACKEND_URL!);
  const state: Proxy = {
    server: createServer((downstream) => {
      if (state.refusing) {
        downstream.resetAndDestroy();
        return;
      }
      const upstream = createConnection({
        host: backend.hostname,
        port: Number(backend.port),
      });
      for (const socket of [downstream, upstream]) {
        state.sockets.add(socket);
        const close = () => {
          state.sockets.delete(socket);
          downstream.destroy();
          upstream.destroy();
        };
        socket.on("error", close);
        socket.on("close", close);
      }
      downstream.pipe(upstream);
      upstream.pipe(downstream);
    }),
    sockets: new Set(),
    refusing: false,
  };
  await new Promise<void>((resolve, reject) => {
    state.server.once("error", reject);
    state.server.listen(0, "127.0.0.1", resolve);
  });
  const address = state.server.address();
  if (address === null || typeof address === "string")
    throw new Error("The recovery proxy has no TCP address");
  const id = String(address.port);
  proxies.set(id, state);
  return { id, url: `http://127.0.0.1:${address.port}` };
}

/** Reset every open connection and refuse new ones until restore. */
function dropRecoveryProxy(_context: unknown, id: string): void {
  const state = proxy(id);
  state.refusing = true;
  for (const socket of state.sockets) socket.resetAndDestroy();
}

function restoreRecoveryProxy(_context: unknown, id: string): void {
  proxy(id).refusing = false;
}

async function closeRecoveryProxy(
  _context: unknown,
  id: string,
): Promise<void> {
  const state = proxies.get(id);
  if (!state) return;
  proxies.delete(id);
  for (const socket of state.sockets) socket.destroy();
  await new Promise<void>((resolve) => state.server.close(() => resolve()));
}

export const recoveryProxyCommands = {
  startRecoveryProxy,
  dropRecoveryProxy,
  restoreRecoveryProxy,
  closeRecoveryProxy,
};

declare module "vitest/browser" {
  interface BrowserCommands {
    startRecoveryProxy(): Promise<{ id: string; url: string }>;
    dropRecoveryProxy(id: string): Promise<void>;
    restoreRecoveryProxy(id: string): Promise<void>;
    closeRecoveryProxy(id: string): Promise<void>;
  }
}
