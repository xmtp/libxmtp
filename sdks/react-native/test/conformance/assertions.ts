export function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

export function equal(
  actual: unknown,
  expected: unknown,
  message: string,
): void {
  assert(actual === expected, message);
}

export function bytesEqual(
  actual: Uint8Array,
  expected: readonly number[],
  message: string,
): void {
  assert(actual instanceof Uint8Array, `${message}: expected Uint8Array`);
  assert(
    actual.length === expected.length &&
      expected.every((value, index) => actual[index] === value),
    message,
  );
}
