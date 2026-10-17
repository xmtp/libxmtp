// Vitest browser commands run this HTTP host in the Node process. It gives
// attachment downloads a redirect, failure statuses, and changed objects.
// Every answer allows CORS, so the browser shows the status to the worker.
import { createHash } from "node:crypto";
import { createServer, type Server } from "node:http";

type Host = {
  server: Server;
  requests: string[];
  objects: Map<string, Buffer>;
};
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
      const object = state.objects.get(path);
      if (object) {
        response.writeHead(200, { "access-control-allow-origin": "*" });
        response.end(object);
        return;
      }
      const answer = answers[path] ?? { status: 500 };
      response.writeHead(answer.status, {
        "access-control-allow-origin": "*",
        ...(answer.location ? { location: answer.location } : {}),
      });
      response.end();
    }),
    requests: [],
    objects: new Map(),
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

/**
 * Serves the object at `source` at `/tampered`, with its last byte changed.
 * The last 16 bytes of an attachment ciphertext are its AES-GCM tag. Returns
 * the SHA-256 hex digest of the changed object, so only the tag check fails.
 */
async function serveTamperedObject(
  _context: unknown,
  id: string,
  source: string,
): Promise<{ path: string; contentDigest: string }> {
  const state = hosts.get(id);
  if (!state) throw new Error(`No download host ${id}`);
  const response = await fetch(source);
  if (!response.ok)
    throw new Error(`The object store answered ${response.status}`);
  const object = Buffer.from(await response.arrayBuffer());
  object[object.length - 1]! ^= 1;
  const path = "/tampered";
  state.objects.set(path, object);
  const contentDigest = createHash("sha256").update(object).digest("hex");
  return { path, contentDigest };
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
  serveTamperedObject,
  closeDownloadHost,
};

declare module "vitest/browser" {
  interface BrowserCommands {
    startDownloadHost(): Promise<{ id: string; url: string }>;
    downloadHostRequests(id: string): Promise<string[]>;
    serveTamperedObject(
      id: string,
      source: string,
    ): Promise<{ path: string; contentDigest: string }>;
    closeDownloadHost(id: string): Promise<void>;
  }
}
