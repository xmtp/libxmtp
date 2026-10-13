import { Errors, Flags } from "@oclif/core";
import { type Client, type PublicIdentity, XmtpError } from "@xmtp/node-sdk";

import { identifierKindMap } from "@/utils/enums";
import {
  asciiLowercase,
  handleNotFoundMessage,
  invalidHandleMessage,
  isValidHandle,
} from "@/utils/handles";

/** Identifier kinds that a command argument can name. */
export const IDENTIFIER_KINDS = ["ethereum", "handle"] as const;

export type IdentifierKind = (typeof IDENTIFIER_KINDS)[number];

/** The `--identifier-kind` flag that commands with identity arguments share. */
export const identifierKindFlag = Flags.option({
  options: IDENTIFIER_KINDS,
  description:
    "The type of identifier: ethereum (an address) or handle (such as nick or nick#2341)",
  default: "ethereum",
});

/**
 * Make an SDK identity from command input. An address is lowercased. A
 * handle goes to the SDK as typed, because the SDK lowercases only ASCII
 * letters and rejects every other character. A handle outside the grammar
 * fails here, with a plain message, before the SDK sees it.
 */
export function publicIdentity(
  identifier: string,
  kind: IdentifierKind,
): PublicIdentity {
  if (kind === "handle" && !isValidHandle(identifier)) {
    throw new Errors.CLIError(invalidHandleMessage(identifier));
  }
  return {
    kind: identifierKindMap[kind],
    identifier: kind === "handle" ? identifier : identifier.toLowerCase(),
  };
}

/**
 * The handles among `identities` that no inbox holds, in canonical text and
 * without repeats. Each lookup uses the verified handle resolver, as the
 * SDK's identity calls do. A lookup that fails counts as found, so the
 * result names only handles that the resolver says no inbox holds.
 */
export async function unresolvedHandles(
  client: Pick<Client, "inboxIdFor">,
  identities: readonly PublicIdentity[],
): Promise<string[]> {
  const handles = [
    ...new Set(
      identities
        .filter((identity) => identity.kind === "handle")
        .map((identity) => asciiLowercase(identity.identifier)),
    ),
  ];
  const missing = await Promise.all(
    handles.map(async (handle) => {
      try {
        const inboxId = await client.inboxIdFor({
          kind: "handle",
          identifier: handle,
        });
        return inboxId === undefined;
      } catch {
        return false;
      }
    }),
  );
  return handles.filter((_, index) => missing[index]);
}

/**
 * Run an SDK call that resolves `identities`. When the call fails with
 * `IdentityNotFound` and a handle among them has no inbox, fail with a plain
 * message that names each such handle. The SDK error does not name the
 * handle, so this looks each handle up again. Every other error passes
 * through.
 */
export async function reportUnresolvedHandles<T>(
  client: Pick<Client, "inboxIdFor">,
  identities: readonly PublicIdentity[],
  call: () => Promise<T>,
): Promise<T> {
  try {
    return await call();
  } catch (error) {
    if (error instanceof XmtpError.IdentityNotFound) {
      const missing = await unresolvedHandles(client, identities);
      if (missing.length > 0) {
        throw new Errors.CLIError(handleNotFoundMessage(missing));
      }
    }
    throw error;
  }
}

/** The key of an identity in a `canMessage` result. */
export function canMessageKey(identity: PublicIdentity): string {
  const text =
    identity.kind === "handle"
      ? asciiLowercase(identity.identifier)
      : identity.identifier;
  return `${identity.kind}:${text}`;
}
