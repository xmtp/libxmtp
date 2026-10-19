// 64-bit SDK values are `bigint`, and `JSON.stringify` throws on them. Persist
// them as decimal strings.

export const INT64_MAX = 9_223_372_036_854_775_807n;

/**
 * The bigint for decimal text written by `bigintReplacer`: an optional `-`
 * and digits, nothing else. Returns undefined for anything else.
 */
export function decimalToBigint(text: string): bigint | undefined {
  return /^-?[0-9]+$/u.test(text) ? BigInt(text) : undefined;
}

/** `JSON.stringify` replacer that writes bigints as decimal strings. */
export function bigintReplacer(_key: string, value: unknown): unknown {
  return typeof value === "bigint" ? value.toString() : value;
}
