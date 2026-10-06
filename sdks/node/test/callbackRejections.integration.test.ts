import { execFile } from "node:child_process";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

import { expect, it } from "vitest";

it("settles all seven async callback routes, keeps later listener/log calls, and ends a client from a log sink", async () => {
  const { stdout, stderr } = await promisify(execFile)(
    process.execPath,
    [
      fileURLToPath(
        new URL("./fixtures/callback-rejections.mjs", import.meta.url),
      ),
    ],
    { env: process.env, timeout: 120000 },
  );
  const result = stdout.trim().split("\n").at(-1);
  expect(JSON.parse(result!)).toMatchObject({
    result: "PASS",
    checks: 36,
    failures: [],
    unhandled: 0,
  });
  expect(stderr).not.toContain("callback-private-rejection-sentinel");
});
