import { describe, expect, it } from "vitest";

import { NETWORK_WAIT, waitForNetwork } from "@/util/test";

describe("waitForNetwork", () => {
  /**
   * The bug this helper exists for: `vi.waitFor` defaults to a 1000 ms timeout
   * and does not inherit `testTimeout`, so every network-backed wait in this
   * suite had one second to complete real MLS work. Encoded as behaviour rather
   * than as an assertion on the constant: a condition that only becomes true
   * after the old default would have expired must still be observed.
   */
  it("waits past vitest's one-second vi.waitFor default", async () => {
    const start = Date.now();
    await waitForNetwork(() => {
      expect(Date.now() - start).toBeGreaterThan(1_500);
    });
  });

  /**
   * Polling must stay well above the 50 ms default. Each poll in this suite runs
   * a real `sync()`, so a tight interval fires ~20 syncs a second at the backend
   * and adds the very load that makes the wait time out.
   */
  it("polls well above the 50 ms default", async () => {
    const start = Date.now();
    let attempts = 0;
    await waitForNetwork(() => {
      attempts += 1;
      expect(attempts).toBe(3);
    });
    // Three attempts span two intervals; the 50 ms default would be ~100 ms.
    expect(Date.now() - start).toBeGreaterThanOrEqual(400);
  });

  /**
   * The timeout must stay under `vitest.config.ts`'s 120 s `testTimeout` so a
   * stuck wait fails with its own condition's assertion instead of an opaque
   * whole-test timeout that names nothing.
   */
  it("times out inside the test budget", () => {
    expect(NETWORK_WAIT.timeout).toBeLessThan(120_000);
  });
});
