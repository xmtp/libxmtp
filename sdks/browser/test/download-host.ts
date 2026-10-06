// Vitest browser commands run this HTTP host in the Node process. It gives
// attachment downloads a redirect and failure statuses. Every answer allows
// CORS, so the browser shows the status to the worker.
import { createServer, type Server } from "node:http";

type Host = { server: Server; requests: string[] };
const hosts = new Map<string, Host>();

/** `/redirect` sends the browser to `/unavailable`. */
const answers: Record<string, { status: number; location?: string }> = {
  "/redirect": { status: 302, location: "/unavailable" },
  "/unavailable": { status: 503 },
  "/gone": { status: 404 },
};

async function startDownloadHost(): Promise<{ id: string; url: string }> {
  const state: Host = {
    server: createServer((request, response) => {
      const path = request.url ?? "";
      state.requests.push(path);
      const answer = answers[path] ?? { status: 500 };
      response.writeHead(answer.status, {
        "access-control-allow-origin": "*",
        ...(answer.location ? { location: answer.location } : {}),
      });
      response.end();
    }),
    requests: [],
  };
  await new Promise<void>((resolve, reject) => {
    state.server.once("error", reject);
    state.server.listen(0, "127.0.0.1", resolve);
  });
  const address = state.server.address();
  if (address === null || typeof address === "string")
    throw new Error("The download host has no TCP address");
  const id = String(address.port);
  hosts.set(id, state);
  return { id, url: `http://127.0.0.1:${address.port}` };
}

/** The paths the host received, in order. */
function downloadHostRequests(_context: unknown, id: string): string[] {
  const state = hosts.get(id);
  if (!state) throw new Error(`No download host ${id}`);
  return [...state.requests];
}

async function closeDownloadHost(_context: unknown, id: string): Promise<void> {
  const state = hosts.get(id);
  if (!state) return;
  hosts.delete(id);
  state.server.closeAllConnections();
  await new Promise<void>((resolve) => state.server.close(() => resolve()));
}

export const downloadHostCommands = {
  startDownloadHost,
  downloadHostRequests,
  closeDownloadHost,
};

declare module "vitest/browser" {
  interface BrowserCommands {
    startDownloadHost(): Promise<{ id: string; url: string }>;
    downloadHostRequests(id: string): Promise<string[]>;
    closeDownloadHost(id: string): Promise<void>;
  }
}
