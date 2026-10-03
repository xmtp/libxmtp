import { execFile } from "node:child_process";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

import { expect, it } from "vitest";

it.each(["off", "otel"])(
  "starts native logging in a fresh process with OTLP %s and accepts repeat init",
  async (mode) => {
    const { stdout } = await promisify(execFile)(
      process.execPath,
      [
        fileURLToPath(
          new URL("./fixtures/logging-startup.mjs", import.meta.url),
        ),
        mode,
      ],
      { env: process.env, timeout: 30000 },
    );
    const results = stdout
      .split(/\r?\n/)
      .filter((line) => line.startsWith('{"result":"PASS","mode":'));
    expect(results).toHaveLength(1);
    expect(JSON.parse(results[0]!)).toMatchObject({
      result: "PASS",
      mode,
    });
  },
);
