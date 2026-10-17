import { describe, expect, it } from "vitest";

import { getValidLogLevels, parseLogLevel } from "@/debug/log";

describe("parseLogLevel", () => {
  it("should parse lowercase log levels", () => {
    expect(parseLogLevel("off")).toBe("off");
    expect(parseLogLevel("error")).toBe("error");
    expect(parseLogLevel("warn")).toBe("warn");
    expect(parseLogLevel("info")).toBe("info");
    expect(parseLogLevel("debug")).toBe("debug");
    expect(parseLogLevel("trace")).toBe("trace");
  });

  it("should parse uppercase log levels", () => {
    expect(parseLogLevel("OFF")).toBe("off");
    expect(parseLogLevel("ERROR")).toBe("error");
    expect(parseLogLevel("WARN")).toBe("warn");
    expect(parseLogLevel("INFO")).toBe("info");
    expect(parseLogLevel("DEBUG")).toBe("debug");
    expect(parseLogLevel("TRACE")).toBe("trace");
  });

  it("should parse properly cased log levels", () => {
    expect(parseLogLevel("Off")).toBe("off");
    expect(parseLogLevel("Error")).toBe("error");
    expect(parseLogLevel("Warn")).toBe("warn");
    expect(parseLogLevel("Info")).toBe("info");
    expect(parseLogLevel("Debug")).toBe("debug");
    expect(parseLogLevel("Trace")).toBe("trace");
  });

  it("should parse mixed case log levels", () => {
    expect(parseLogLevel("dEBUG")).toBe("debug");
    expect(parseLogLevel("WaRn")).toBe("warn");
  });

  it("should return null for invalid log levels", () => {
    expect(parseLogLevel("invalid")).toBeNull();
    expect(parseLogLevel("")).toBeNull();
    expect(parseLogLevel("verbose")).toBeNull();
    expect(parseLogLevel("warning")).toBeNull();
  });
});

describe("getValidLogLevels", () => {
  it("should return all valid log levels", () => {
    const levels = getValidLogLevels();
    expect(levels).toContain("off");
    expect(levels).toContain("error");
    expect(levels).toContain("warn");
    expect(levels).toContain("info");
    expect(levels).toContain("debug");
    expect(levels).toContain("trace");
    expect(levels).toHaveLength(6);
  });

  it("should return a new array each time", () => {
    const levels1 = getValidLogLevels();
    const levels2 = getValidLogLevels();
    expect(levels1).not.toBe(levels2);
    expect(levels1).toEqual(levels2);
  });
});
