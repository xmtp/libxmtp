import { homedir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import {
  backendLabel,
  defaultDbPath,
  parseBackendUrl,
} from "../../src/utils/backend.js";

describe("backend utilities", () => {
  it.each([
    ["http://example.com", "example-com-80-"],
    ["https://example.com", "example-com-443-"],
    ["http://example.com:5050/path", "example-com-5050-"],
  ])("labels %s", (url, prefix) => {
    expect(backendLabel(url)).toMatch(new RegExp(`^${prefix}[a-f0-9]{6}$`));
  });

  it("keeps sanitized origins collision free", () => {
    expect(backendLabel("http://a_b:5050")).not.toBe(
      backendLabel("http://a-b:5050"),
    );
  });

  it("derives the default database path", () => {
    const url = "https://example.com";
    expect(defaultDbPath(url)).toBe(
      join(homedir(), ".xmtp", backendLabel(url), "xmtp-db"),
    );
  });

  it.each(["http://", "https://[", "ftp://example.com"])(
    "rejects invalid backend %s",
    (url) => {
      expect(() => parseBackendUrl(url)).toThrow(
        "Backend URL must be a valid http:// or https:// URL.",
      );
    },
  );
});
