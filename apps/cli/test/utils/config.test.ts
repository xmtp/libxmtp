import { describe, expect, it } from "vitest";

import { mergeConfig } from "@/utils/config";

describe("merged CLI storage config", () => {
  it("uses the effective environment label for generated paths", () => {
    const backendUrl = "https://example.com";
    const local = mergeConfig({ backendUrl, env: "local" }, {});
    const staging = mergeConfig(
      { backendUrl, env: "local" },
      {
        env: "staging-a",
      },
    );
    expect(staging.dbPath).not.toBe(local.dbPath);
    expect(staging.dbPath).toContain("staging-a");
  });

  it("keeps an explicit database path when the environment changes", () => {
    const dbPath = "/tmp/explicit-xmtp.db3";
    expect(
      mergeConfig(
        { backendUrl: "https://example.com", dbPath, env: "local" },
        { env: "staging-a" },
      ).dbPath,
    ).toBe(dbPath);
  });
});
