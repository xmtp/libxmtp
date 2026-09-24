import { test } from "vitest";

test("scenarios 1, 2, and 7 through @ubjs/node", async () => {
  await import("./node.mts");
}, 120_000);
