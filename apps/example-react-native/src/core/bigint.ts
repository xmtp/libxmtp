// 64-bit SDK values are `bigint`, and `JSON.stringify` throws on them. Persist
// them as decimal strings.

export const INT64_MAX = 9_223_372_036_854_775_807n;
export const INT64_MIN = -9_223_372_036_854_775_808n;

/** The decimal text of `value` (for JSON). */
export function bigintToDecimal(value: bigint): string {
  return value.toString();
}

/**
 * The bigint for decimal text written by `bigintToDecimal`: an optional `-`
 * and digits, nothing else. Returns undefined for anything else.
 */
export function decimalToBigint(text: string): bigint | undefined {
  return /^-?[0-9]+$/u.test(text) ? BigInt(text) : undefined;
}

/** True when `value` fits in a signed 64-bit integer. */
export function isInt64(value: bigint): boolean {
  return value >= INT64_MIN && value <= INT64_MAX;
}

/** Milliseconds for nanoseconds, rounded down (for `Date`). */
export function nanosecondsToMilliseconds(ns: bigint): number {
  return Number(ns / 1_000_000n);
}

/** `JSON.stringify` replacer that writes bigints as decimal strings. */
export function bigintReplacer(_key: string, value: unknown): unknown {
  return typeof value === "bigint" ? value.toString() : value;
}
