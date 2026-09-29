import assert from "node:assert/strict";

export async function assertNoUnhandledRejection(
  action: () => Promise<void>,
): Promise<void> {
  const unhandled: unknown[] = [];
  const capture = (error: unknown): void => {
    unhandled.push(error);
  };
  process.on("unhandledRejection", capture);
  try {
    await action();
    await new Promise((resolve) => setImmediate(resolve));
    assert.deepEqual(unhandled, [], "stream left an unhandled rejection");
  } finally {
    process.off("unhandledRejection", capture);
  }
}
