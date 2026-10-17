import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const INVALID_BACKEND_URL =
  "Backend URL must be a valid http:// or https:// URL.";
const INVALID_ENVIRONMENT_LABEL =
  'Environment label must be non-empty, must not be "." or "..", and must not contain "/", "\\", ":", or NUL.';

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

export function parseEnvironmentLabel(
  value: string,
  platform: NodeJS.Platform = process.platform,
): string {
  if (
    value.length === 0 ||
    value === "." ||
    value === ".." ||
    value.includes("/") ||
    value.includes("\\") ||
    value.includes(":") ||
    value.includes("\0")
  ) {
    throw new Error(INVALID_ENVIRONMENT_LABEL);
  }
  if (platform === "win32" && /[. ]$/.test(value))
    throw new Error(
      `${INVALID_ENVIRONMENT_LABEL} On Windows, it must not end in a dot or space.`,
    );

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

export function defaultDbPath(
  value: string,
  environmentLabel = "local",
  homeDirectory = homedir(),
): string {
  const label = parseEnvironmentLabel(environmentLabel);
  const backendDirectory = join(homeDirectory, ".xmtp", backendLabel(value));
  const oldPath = join(backendDirectory, "xmtp-db");
  if (label === "local") return oldPath;

  const labeledPath = join(backendDirectory, "environments", label, "xmtp-db");
  if (existsSync(oldPath)) {
    if (existsSync(labeledPath))
      throw new Error(
        "Both legacy and labeled CLI databases exist. Set XMTP_DB_PATH to select one.",
      );
    return oldPath;
  }
  return labeledPath;
}
