import { Errors, Flags } from "@oclif/core";
import type { PublicIdentity } from "@xmtp/node-sdk";

import { identifierKindMap } from "@/utils/enums";
import {
  asciiLowercase,
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

/** The key of an identity in a `canMessage` result. */
export function canMessageKey(identity: PublicIdentity): string {
  const text =
    identity.kind === "handle"
      ? asciiLowercase(identity.identifier)
      : identity.identifier;
  return `${identity.kind}:${text}`;
}
