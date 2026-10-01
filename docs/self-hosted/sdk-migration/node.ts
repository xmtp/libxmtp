import { resolve } from "node:path";

import {
  Client,
  type ClientOptions,
  type ErrorDetails,
  type Signer,
} from "xmtp-sdk";

// End the old SDK client before calling this function.
export async function exerciseMigration(
  signer: Signer,
  options: ClientOptions,
  dbPath: string,
  attachmentsDir: string,
): Promise<void> {
  const expectedPath = resolve(dbPath);
  const explicit: ClientOptions = {
    ...options,
    storage: {
      ...options.storage,
      location: { dbPath, attachmentsDir },
    },
  };
  const first = await Client.create(signer, explicit);
  const identity = first.identity;
  const inboxId = first.inboxId;
  const openedPath = await first.storage.path();
  await first.end();
  if (openedPath === undefined || resolve(openedPath) !== expectedPath)
    throw new Error("The SDK opened another database");

  const reopened = await Client.build(identity, {
    ...explicit,
    allowOffline: true,
  });
  try {
    if (reopened.inboxId !== inboxId) throw new Error("The inbox changed");
    const reopenedPath = await reopened.storage.path();
    if (reopenedPath === undefined || resolve(reopenedPath) !== expectedPath)
      throw new Error("The reopened database path changed");
  } finally {
    await reopened.end();
  }
}

// Keep a default case for codes added after this app is built.
export function describeMigrationError(details: ErrorDetails): string {
  switch (details.code) {
    case "IdentityNotFound":
      return "Use the existing database or create with a signer";
    case "IdentityMismatch":
      return "Use the identity that belongs to this database";
    case "ForeignCursor":
    case "InvalidCursor":
      return "Use an unchanged cursor from this database";
    case "CodecNotFound":
      return "Register the matching codec or display fallback";
    case "CodecDecodeFailed":
    case "MalformedEnvelope":
      return "Keep the original bytes and report the content failure";
    default:
      return `${details.code}: ${details.message}; retryable=${details.retryable}`;
  }
}
