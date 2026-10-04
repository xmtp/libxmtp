import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { dirname, join } from "node:path";

import { describe, expect, it } from "vitest";

import {
  backendLabel,
  defaultDbPath,
  parseBackendUrl,
  parseEnvironmentLabel,
} from "@/utils/backend";

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

  it.each([
    "http://example.com",
    "https://example.com",
    "http://example.com:5050/path",
    "http://a_b:5050",
    "http://a-b:5050",
  ])("derives the default database path for %s", (url) => {
    expect(defaultDbPath(url)).toBe(
      join(homedir(), ".xmtp", backendLabel(url), "xmtp-db"),
    );
  });

  it("keeps sanitized origin collisions on distinct paths", () => {
    expect(defaultDbPath("http://a_b:5050")).not.toBe(
      defaultDbPath("http://a-b:5050"),
    );
  });

  it("keeps non-local environment labels in separate database paths", () => {
    const url = "https://example.com";
    expect(defaultDbPath(url, "local")).toBe(defaultDbPath(url));
    expect(defaultDbPath(url, "staging-a")).toBe(
      join(
        homedir(),
        ".xmtp",
        backendLabel(url),
        "environments",
        "staging-a",
        "xmtp-db",
      ),
    );
    expect(defaultDbPath(url, "staging-b")).not.toBe(
      defaultDbPath(url, "staging-a"),
    );
    expect(defaultDbPath(url, "xmtp-db")).not.toBe(defaultDbPath(url));
  });

  it("reuses an existing CLI database and rejects two possible paths", () => {
    const home = mkdtempSync(join(tmpdir(), "xmtp-cli-legacy-"));
    const url = "https://example.com";
    const oldPath = join(home, ".xmtp", backendLabel(url), "xmtp-db");
    const newPath = join(
      home,
      ".xmtp",
      backendLabel(url),
      "environments",
      "production",
      "xmtp-db",
    );
    try {
      mkdirSync(dirname(oldPath), { recursive: true });
      writeFileSync(oldPath, "old database");
      expect(defaultDbPath(url, "production", home)).toBe(oldPath);
      mkdirSync(dirname(newPath), { recursive: true });
      writeFileSync(newPath, "new database");
      expect(() => defaultDbPath(url, "production", home)).toThrow(
        "Both legacy and labeled CLI databases exist",
      );
    } finally {
      rmSync(home, { recursive: true, force: true });
    }
  });

  it.each([
    ["http://example.com", "http://example.com/"],
    ["http://example.com", "http://example.com/path"],
    ["http://example.com", "http://example.com:80"],
    ["https://example.com", "https://example.com:443"],
  ])("uses equivalent paths for %s and %s", (left, right) => {
    expect(defaultDbPath(left)).toBe(defaultDbPath(right));
  });

  it.each(["http://", "https://[", "ftp://example.com"])(
    "rejects invalid backend %s",
    (url) => {
      expect(() => parseBackendUrl(url)).toThrow(
        "Backend URL must be a valid http:// or https:// URL.",
      );
    },
  );

  it.each(["", ".", "..", "a/b", "a\\b", "a:b", "a\0b"])(
    "rejects invalid environment label %s",
    (label) => {
      expect(() => parseEnvironmentLabel(label)).toThrow(
        "Environment label must be non-empty",
      );
    },
  );

  it.each(["name.", "name "])(
    "rejects a Windows environment label ending in a dot or space: %s",
    (label) => {
      expect(() => parseEnvironmentLabel(label, "win32")).toThrow(
        "Environment label must be non-empty",
      );
      expect(parseEnvironmentLabel(label, "darwin")).toBe(label);
    },
  );
});
