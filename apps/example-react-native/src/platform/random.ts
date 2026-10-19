import * as Crypto from "expo-crypto";

/** `count` bytes from the platform CSPRNG (Hermes has no `crypto.getRandomValues`). */
export function randomBytes(count: number): Uint8Array {
  return Crypto.getRandomBytes(count);
}

/** A random version 4 UUID, uppercase like Foundation's `UUID().uuidString`. */
export function randomUuid(): string {
  return Crypto.randomUUID().toUpperCase();
}
