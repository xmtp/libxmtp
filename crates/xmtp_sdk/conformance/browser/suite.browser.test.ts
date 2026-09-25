import { expect, test } from "vitest";

import { runBrowserBridgeConformance } from "./suite.chromium";

declare const __XMTP_BACKEND_URL__: string;

test.todo(
  "scenario 7: Task 19 adds acknowledged streams and idle-read cancel (O2)",
);
test.todo(
  "scenario 8: Task 20 adds event and listener methods to the bridge (O2)",
);

test("browser bridge scenarios 1 to 11 and worker smoke checks", async () => {
  const results = await runBrowserBridgeConformance(__XMTP_BACKEND_URL__);
  for (const result of results) console.log(result);
  for (const scenario of [1, 2, 3, 4, 5, 6, 9, 10, 11]) {
    expect(
      results.some((line) => line.startsWith(`scenario ${scenario}:`)),
    ).toBe(true);
  }
  expect(results.filter((line) => line.startsWith("PENDING"))).toHaveLength(2);
  expect(results.filter((line) => line.startsWith("smoke:"))).toHaveLength(6);
}, 180_000);
