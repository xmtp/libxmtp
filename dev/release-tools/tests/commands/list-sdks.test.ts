import { describe, it, expect } from "vitest";
import { listSdksForChannel } from "../../src/commands/list-sdks";

describe("listSdksForChannel", () => {
  it("returns fan-out targets for nightly, excluding the hub", () => {
    const rows = listSdksForChannel("nightly");
    const names = rows.map((r) => r.sdk).sort();
    expect(names).toEqual(["android", "ios", "node-bindings", "wasm-bindings"]);
    // hub (libxmtp, empty releaseWorkflow) is excluded
    expect(rows.every((r) => r.releaseWorkflow !== "")).toBe(true);
  });

  it("each row carries the data the fan-out needs", () => {
    const ios = listSdksForChannel("nightly").find((r) => r.sdk === "ios");
    expect(ios).toMatchObject({
      sdk: "ios",
      releaseWorkflow: "release-ios.yml",
      versionTrack: "independent",
    });
  });
});
