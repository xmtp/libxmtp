// Backend URL normalization (ExampleCore/Session/BackendURL.swift). A hand
// parser, because React Native's global URL is a partial polyfill.

import { SessionCopy, SessionError } from "./sessionError.ts";

/** A canonical backend URL: the profile key for one server. */
export interface BackendUrl {
  /** Lowercase scheme and host, no default port, no trailing `/`. */
  readonly value: string;
  /** True only for a loopback host. Gates private attachment URLs. */
  readonly allowPrivateNetwork: boolean;
}

/** Hosts that count as loopback. `127.0.0.2`, `0.0.0.0` and `10.0.2.2` do not. */
const LOOPBACK_HOSTS = new Set(["localhost", "127.0.0.1", "::1", "[::1]"]);

const SCHEME = /^([A-Za-z][A-Za-z0-9+.-]*):\/\/(.*)$/su;
const REG_NAME = /^[A-Za-z0-9\-._~!$&'()*+,;=%]+$/u;
const IP_LITERAL = /^\[[0-9A-Fa-f:.]+\]$/u;
const PATH = /^[A-Za-z0-9\-._~!$&'()*+,;=:@%/]*$/u;

interface Parts {
  readonly scheme: string;
  readonly host: string;
  readonly port: string | undefined;
  readonly path: string;
  readonly userInfo: boolean;
  readonly query: boolean;
  readonly fragment: boolean;
}

/** Splits an absolute URL, or returns undefined when it does not parse. */
function split(text: string): Parts | undefined {
  const match = SCHEME.exec(text);
  if (match === null) return undefined;
  const [, scheme = "", rest = ""] = match;
  const end = rest.search(/[/?#]/u);
  const authority = end < 0 ? rest : rest.slice(0, end);
  let tail = end < 0 ? "" : rest.slice(end);
  const hash = tail.indexOf("#");
  const fragment = hash >= 0;
  if (fragment) tail = tail.slice(0, hash);
  const question = tail.indexOf("?");
  const query = question >= 0;
  const path = query ? tail.slice(0, question) : tail;
  if (!PATH.test(path)) return undefined;

  const at = authority.lastIndexOf("@");
  const hostPort = at < 0 ? authority : authority.slice(at + 1);
  let host: string;
  let port: string | undefined;
  if (hostPort.startsWith("[")) {
    const close = hostPort.indexOf("]");
    if (close < 0) return undefined;
    host = hostPort.slice(0, close + 1);
    const after = hostPort.slice(close + 1);
    if (after !== "" && !after.startsWith(":")) return undefined;
    port = after === "" ? undefined : after.slice(1);
    if (!IP_LITERAL.test(host)) return undefined;
  } else {
    const colon = hostPort.indexOf(":");
    host = colon < 0 ? hostPort : hostPort.slice(0, colon);
    port = colon < 0 ? undefined : hostPort.slice(colon + 1);
    if (host !== "" && !REG_NAME.test(host)) return undefined;
  }
  if (port !== undefined && port !== "" && !/^\d+$/u.test(port)) {
    return undefined;
  }
  return {
    scheme: scheme.toLowerCase(),
    host,
    port,
    path,
    userInfo: at >= 0,
    query,
    fragment,
  };
}

/**
 * Normalizes a backend address, or throws a `SessionError` with the iOS copy.
 *
 * Trims whitespace, removes every trailing `/`, requires `http` or `https`
 * with a host, rejects credentials, a query, a fragment and a port outside
 * 1-65535 (or an empty one), requires HTTPS for a non-loopback host,
 * lowercases the scheme and host, and drops the scheme's default port. The
 * path keeps its case.
 */
export function normalizeBackendUrl(input: string): BackendUrl {
  let text = input.trim();
  while (text.endsWith("/")) text = text.slice(0, -1);
  const parts = split(text);
  if (
    parts === undefined ||
    (parts.scheme !== "http" && parts.scheme !== "https") ||
    parts.host === ""
  ) {
    throw SessionError.message(SessionCopy.invalidUrl);
  }
  const port = parts.port === undefined ? undefined : Number(parts.port);
  if (
    parts.userInfo ||
    parts.query ||
    parts.fragment ||
    parts.port === "" ||
    (port !== undefined && (port < 1 || port > 65535))
  ) {
    throw SessionError.message(SessionCopy.urlParts);
  }
  const host = parts.host.toLowerCase();
  const local = LOOPBACK_HOSTS.has(host);
  if (parts.scheme !== "https" && !local) {
    throw SessionError.message(SessionCopy.remoteHttp);
  }
  const defaultPort = parts.scheme === "https" ? 443 : 80;
  const portText =
    port === undefined || port === defaultPort ? "" : `:${String(port)}`;
  return {
    value: `${parts.scheme}://${host}${portText}${parts.path}`,
    allowPrivateNetwork: local,
  };
}

/** `normalizeBackendUrl`, or undefined instead of an error. */
export function parseBackendUrl(input: string): BackendUrl | undefined {
  try {
    return normalizeBackendUrl(input);
  } catch {
    return undefined;
  }
}

/**
 * The Connect screen's "Connect to" address: the normalized URL with its
 * default port written out (`https://example.com` → `https://example.com:443`).
 */
export function connectToAddress(input: string): string | undefined {
  const url = parseBackendUrl(input);
  if (url === undefined) return undefined;
  const parts = split(url.value);
  if (parts === undefined) return undefined;
  const port = parts.port ?? (parts.scheme === "https" ? "443" : "80");
  return `${parts.scheme}://${parts.host}:${port}${parts.path}`;
}

/** True when the host of a normalized URL is loopback. */
export function isLoopbackUrl(input: string): boolean {
  return parseBackendUrl(input)?.allowPrivateNetwork ?? false;
}
