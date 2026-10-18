// Recipient input: a handle, an Ethereum address or an inbox ID
// (ExampleCore/Identity/IdentityInput.swift and NewConversationStore.swift).

import { handleProblem, normalizeHandle } from "./handles.ts";

export type Recipient =
  | { readonly kind: "inbox"; readonly inboxId: string }
  | { readonly kind: "ethereum"; readonly address: string }
  | { readonly kind: "handle"; readonly handle: string };

export const RecipientCopy = {
  empty: "Enter a handle, Ethereum address, or inbox ID.",
  invalidAddress:
    "Enter an Ethereum address with 0x followed by 40 hexadecimal characters.",
  invalidInbox: "An inbox ID must contain 64 hexadecimal characters.",
  invalidHandle:
    "Enter a handle such as nick or nick#2341, an Ethereum address, or an inbox ID.",
  handleNotFound: "No XMTP account has this handle.",
  addressNotFound: "This address has no XMTP account.",
} as const;

export type RecipientProblem = keyof typeof RecipientCopy;

/** A recipient that does not parse or resolve. Its message is the iOS copy. */
export class RecipientError extends Error {
  readonly problem: RecipientProblem;

  constructor(problem: RecipientProblem) {
    super(RecipientCopy[problem]);
    this.name = "RecipientError";
    this.problem = problem;
  }
}

const HEX = /^[0-9a-fA-F]*$/u;

/** Character count like Swift's `String.count` for these inputs (code points). */
const length = (text: string): number => Array.from(text).length;

/**
 * Parses one recipient. After trimming: `0x…` must be 42 characters of hex
 * (lowercased); exactly 64 characters must be hex (an inbox ID, lowercased);
 * anything else is a handle without one leading `@`, ASCII-lowercased, that
 * passes the handle grammar with a minimum of 1.
 */
export function parseRecipient(input: string): Recipient {
  const value = input.trim();
  if (value === "") throw new RecipientError("empty");
  if (value.toLowerCase().startsWith("0x")) {
    if (length(value) !== 42 || !HEX.test(value.slice(2))) {
      throw new RecipientError("invalidAddress");
    }
    return { kind: "ethereum", address: value.toLowerCase() };
  }
  if (length(value) === 64) {
    if (!HEX.test(value)) throw new RecipientError("invalidInbox");
    return { kind: "inbox", inboxId: value.toLowerCase() };
  }
  const handle = normalizeHandle(
    value.startsWith("@") ? value.slice(1) : value,
  );
  if (handle === "" || handleProblem(handle, 1) !== undefined) {
    throw new RecipientError("invalidHandle");
  }
  return { kind: "handle", handle };
}

/** The not-found error for a recipient that did not resolve. */
export function recipientNotFound(recipient: Recipient): RecipientError {
  return new RecipientError(
    recipient.kind === "handle" ? "handleNotFound" : "addressNotFound",
  );
}

/** The comma-separated entries of a recipient field; empty entries are dropped. */
export function recipientEntries(input: string): readonly string[] {
  return input.split(",").filter((entry) => entry.length > 0);
}

/** A resolved recipient's label: `@handle`, else the first 12 inbox ID characters. */
export function recipientLabel(inboxId: string, handle?: string): string {
  return handle === undefined ? inboxId.slice(0, 12) : "@" + handle;
}
