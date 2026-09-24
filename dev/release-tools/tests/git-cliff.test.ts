import { describe, it, expect } from "vitest";
import { parsePendingFromContext } from "../src/lib/git-cliff";

// `git cliff --bump --context` emits an array of release objects; the first
// (unreleased) entry is what we read. Shapes below match real git-cliff
// output (v-prefixed version, bump_type, nested previous.version); extra
// fields elided.
const CONTEXT_MINOR = JSON.stringify([
  {
    version: "v1.11.0",
    bump_type: "minor",
    previous: { version: "v1.10.0" },
    commits: [{ id: "abc", message: "feat: x" }],
  },
]);

const CONTEXT_NO_BUMP = JSON.stringify([
  {
    version: "v1.10.0",
    bump_type: null,
    previous: { version: "v1.10.0" },
    commits: [],
  },
]);

describe("parsePendingFromContext", () => {
  it("uses git-cliff's bump_type and strips the v prefix", () => {
    expect(parsePendingFromContext(CONTEXT_MINOR, "1.10.0")).toEqual({
      version: "1.11.0",
      kind: "minor",
    });
  });

  it("returns null when nothing is pending (version == previous, bump_type null)", () => {
    expect(parsePendingFromContext(CONTEXT_NO_BUMP, "1.10.0")).toBeNull();
  });

  it("falls back to diffBumpKind when bump_type is absent", () => {
    const ctx = JSON.stringify([
      { version: "2.0.0", previous: { version: "1.10.0" } },
    ]);
    expect(parsePendingFromContext(ctx, "1.10.0")).toEqual({
      version: "2.0.0",
      kind: "major",
    });
  });

  it("returns null when the context has no releases", () => {
    expect(parsePendingFromContext(JSON.stringify([]), "1.10.0")).toBeNull();
  });

  it("returns null when the first release has no version", () => {
    expect(
      parsePendingFromContext(JSON.stringify([{ commits: [] }]), "1.10.0"),
    ).toBeNull();
  });

  it("throws on unparseable input", () => {
    expect(() => parsePendingFromContext("not json", "1.10.0")).toThrow();
  });

  it("throws on an invalid computed version", () => {
    const ctx = JSON.stringify([
      { version: "not-a-version", bump_type: "minor" },
    ]);
    expect(() => parsePendingFromContext(ctx, "1.10.0")).toThrow();
  });

  it("throws when bump_type is absent and the computed version is not > lastShipped", () => {
    // Defensive: git-cliff is trusted to compute a forward version, but if it
    // ever emitted a version <= lastShipped without a bump_type, the diffBumpKind
    // fallback must fail loudly rather than silently mis-derive a kind.
    const ctx = JSON.stringify([
      { version: "1.9.0", previous: { version: "1.8.0" } },
    ]);
    expect(() => parsePendingFromContext(ctx, "1.10.0")).toThrow();
  });
});
