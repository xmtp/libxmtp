const INVALID_BACKEND_URL =
  "Backend URL must be a valid http:// or https:// URL.";

export const parseBackendUrl = (value: string): URL => {
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
};

export const isValidBackendUrl = (value: string): boolean => {
  try {
    parseBackendUrl(value);
    return true;
  } catch {
    return false;
  }
};

export const backendHost = (value: string): string =>
  parseBackendUrl(value).host;

export const backendLabel = async (value: string): Promise<string> => {
  const url = parseBackendUrl(value);
  const port = url.port || (url.protocol === "https:" ? "443" : "80");
  const hostPort = `${url.hostname}-${port}`.replace(/[^A-Za-z0-9-]/g, "-");
  const digest = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(url.origin),
  );
  const hash = Array.from(new Uint8Array(digest))
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("")
    .slice(0, 6);
  return `${hostPort}-${hash}`;
};
