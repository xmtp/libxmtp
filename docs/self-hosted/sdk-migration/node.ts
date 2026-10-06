import { resolve } from "node:path";

import {
  Client,
  type ClientOptions,
  type ErrorDetails,
  type PublicIdentity,
} from "@xmtp/node-sdk";

// End the old SDK client before calling this function.
export async function exerciseMigration(
  existingIdentity: PublicIdentity,
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
  const first = await Client.build(existingIdentity, explicit);
  const { identity, inboxId, openedPath } = await withMigrationClient(
    first,
    async () => ({
      identity: first.identity,
      inboxId: first.inboxId,
      openedPath: await first.storage.path(),
    }),
  );
  if (openedPath === undefined || resolve(openedPath) !== expectedPath)
    throw new Error("The SDK opened another database");

  const reopened = await Client.build(identity, {
    ...explicit,
    allowOffline: true,
  });
  await withMigrationClient(reopened, async () => {
    if (reopened.inboxId !== inboxId) throw new Error("The inbox changed");
    const reopenedPath = await reopened.storage.path();
    if (reopenedPath === undefined || resolve(reopenedPath) !== expectedPath)
      throw new Error("The reopened database path changed");
  });
}

async function withMigrationClient<T>(
  client: Client,
  action: () => Promise<T>,
): Promise<T> {
  let actionFailed = false;
  try {
    return await action();
  } catch (error) {
    actionFailed = true;
    throw error;
  } finally {
    try {
      await client.end();
    } catch (error) {
      if (!actionFailed) throw error;
    }
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
