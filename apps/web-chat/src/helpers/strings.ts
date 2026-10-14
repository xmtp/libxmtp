import { isValidHandle } from "@/helpers/handles";

/**
 * Lowercases ASCII letters only, as the protocol does with handle input.
 * `toLowerCase` also folds other letters: U+212A KELVIN SIGN becomes ASCII
 * `k`, so `\u212Aate` would pass the handle grammar as `kate`. Each other
 * character stays as typed, so the grammar check refuses it. Ethereum
 * addresses and inbox IDs are ASCII, so this is safe for them too.
 */
export const asciiLowercase = (value: string): string =>
  value.replace(/[A-Z]/g, (letter) => letter.toLowerCase());

/**
 * Normalizes a field that takes an Ethereum address, an inbox ID, or a
 * handle: ASCII capitals become lowercase, and white space is removed.
 * A displayed @ prefix is removed only when the rest is a valid handle.
 */
export const normalizeMemberInput = (value: string): string => {
  const normalized = asciiLowercase(value).replace(/\s/g, "");
  return normalized.startsWith("@") && isValidHandle(normalized.slice(1))
    ? normalized.slice(1)
    : normalized;
};

export const isValidEthereumAddress = (
  address: string,
): address is `0x${string}` => /^0x[a-fA-F0-9]{40}$/.test(address);

export const isValidInboxId = (inboxId: string): inboxId is string =>
  /^[a-f0-9]{64}$/.test(inboxId);

export const shortAddress = (address: string, length: number = 4): string => {
  if (!isValidEthereumAddress(address) && !isValidInboxId(address)) {
    return address;
  }
  return `${address.substring(0, length + 2)}...${address.substring(address.length - length)}`;
};

export const MEMBER_NO_LONGER_IN_GROUP =
  "This member is no longer in the group";

export const jsonStringify = (value: unknown): string =>
  JSON.stringify(
    value,
    (_, value: unknown) =>
      typeof value === "bigint" ? value.toString() : value,
    2,
  );
