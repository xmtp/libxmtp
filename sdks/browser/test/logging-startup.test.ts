import {
  initLogging,
  localSignerFromPrivateKey,
  setLogSink,
} from "@xmtp/browser-sdk";
import { expect, test } from "vitest";

// Each test file has its own page, so logging starts here before any client
// or worker call. A short private key makes the worker log an error.
async function failedCall(): Promise<void> {
  await expect(localSignerFromPrivateKey(new Uint8Array(31))).rejects.toThrow();
}

test("logging starts in a fresh page, feeds the app sink, and accepts a repeat init", async () => {
  await initLogging({ level: "error" });
  const records: string[] = [];
  await setLogSink({
    async log(record) {
      records.push(record.message);
    },
  });
  try {
    await failedCall();
    await expect
      .poll(() => records.length, { timeout: 5_000 })
      .toBeGreaterThan(0);

    const before = records.length;
    await initLogging({ level: "debug" });
    await failedCall();
    await expect
      .poll(() => records.length, { timeout: 5_000 })
      .toBeGreaterThan(before);
  } finally {
    await setLogSink(undefined);
  }
  const after = records.length;
  await failedCall();
  // The worker sends a record after the sink changes only by mistake.
  await new Promise((resolve) => setTimeout(resolve, 200));
  expect(records).toHaveLength(after);
});
