import {
  Client,
  type ClientOptions,
  type ErrorDetails,
  type PublicIdentity,
} from "@xmtp/browser-sdk";
import {
  initPureWasm,
  ReactionV2Codec,
  TextCodec,
} from "@xmtp/browser-sdk/pure";

// Initialize the pure WASM module before constructing a standalone codec.
export async function exercisePureCodecs(): Promise<void> {
  await initPureWasm();
  const text = new TextCodec();
  if (text.decode(text.encode("migration")) !== "migration")
    throw new Error("The text changed");
  const reaction = new ReactionV2Codec();
  const value = {
    kind: "reaction" as const,
    reference: "00".repeat(32),
    referenceInboxId: "11".repeat(32),
    reaction: {
      action: "added" as const,
      schema: "unicode" as const,
      content: "👍",
    },
  };
  if (
    reaction.decode(reaction.encode(value)).reaction.content !==
    value.reaction.content
  )
    throw new Error("The reaction changed");
}

// End the old SDK client before calling this function.
export async function exerciseMigration(
  existingIdentity: PublicIdentity,
  options: ClientOptions,
  dbPath: string,
  attachmentsDir: string,
): Promise<void> {
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
  if (openedPath !== dbPath) throw new Error("The SDK opened another database");

  const reopened = await Client.build(identity, {
    ...explicit,
    allowOffline: true,
  });
  await withMigrationClient(reopened, async () => {
    if (reopened.inboxId !== inboxId) throw new Error("The inbox changed");
    if ((await reopened.storage.path()) !== dbPath)
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
