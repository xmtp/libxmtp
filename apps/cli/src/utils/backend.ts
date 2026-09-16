import { createHash } from "node:crypto";
import { homedir } from "node:os";
import { join } from "node:path";

const INVALID_BACKEND_URL =
  "Backend URL must be a valid http:// or https:// URL.";
const INVALID_ENVIRONMENT_LABEL =
  'Environment label must be non-empty, must not be "." or "..", and must not contain "/" or "\\".';

export function parseBackendUrl(value: string): URL {
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    throw new Error(INVALID_BACKEND_URL);
  }

  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new Error(INVALID_BACKEND_URL);
  }

  return url;
}

export function parseEnvironmentLabel(value: string): string {
  if (
    value.length === 0 ||
    value === "." ||
    value === ".." ||
    value.includes("/") ||
    value.includes("\\")
  ) {
    throw new Error(INVALID_ENVIRONMENT_LABEL);
  }

  return value;
}

export function backendLabel(value: string): string {
  const url = parseBackendUrl(value);
  const port = url.port || (url.protocol === "https:" ? "443" : "80");
  const hostPort = `${url.hostname}-${port}`.replace(/[^A-Za-z0-9-]/g, "-");
  const hash = createHash("sha256")
    .update(url.origin)
    .digest("hex")
    .slice(0, 6);
  return `${hostPort}-${hash}`;
}

export function defaultDbPath(value: string): string {
  return join(homedir(), ".xmtp", backendLabel(value), "xmtp-db");
}
