// Access token handling: the bearer credential the SDK attaches to requests.

import type { Credential } from "@xmtp/react-native-sdk";

import { SessionCopy, SessionError } from "./sessionError.ts";

/** `Int64.max`: the credential never expires; the SDK asks again when it wants. */
export const NEVER_EXPIRES_SECONDS = 9_223_372_036_854_775_807n;

/**
 * The `Authorization` value for a stored token: trimmed, with `Bearer `
 * added unless the token already starts with it (any case). An empty token
 * throws "Enter an access token.".
 */
export function bearerHeader(token: string): string {
  const value = token.trim();
  if (value === "") throw SessionError.message(SessionCopy.enterToken);
  return value.toLowerCase().startsWith("bearer ") ? value : "Bearer " + value;
}

/** The SDK credential for a stored token (no name, never expires). */
export function bearerCredential(token: string): Credential {
  return {
    value: bearerHeader(token),
    expiresAtSeconds: NEVER_EXPIRES_SECONDS,
  };
}
