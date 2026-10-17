import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

const repoRoot = fileURLToPath(new URL("../../../", import.meta.url));

function jobs(file: string): Map<string, string> {
  const yaml = fs.readFileSync(path.join(repoRoot, file), "utf8");
  const body = yaml.split("\njobs:\n")[1];
  if (!body) throw new Error(`No jobs in ${file}`);
  const starts = [...body.matchAll(/^  ([a-zA-Z0-9_-]+):\n/gm)];
  return new Map(
    starts.map((start, index) => [
      start[1],
      body.slice(start.index, starts[index + 1]?.index ?? body.length),
    ]),
  );
}

function permissions(job: string): Map<string, number> {
  const block = job.split("    permissions:\n")[1]?.split(/^    \S/m)[0];
  if (!block) throw new Error("Missing explicit job permissions");
  const level = { none: 0, read: 1, write: 2 };
  return new Map(
    [
      ...block.matchAll(/^      ([a-z-]+): (none|read|write)(?:\s+#.*)?$/gm),
    ].map((entry) => [entry[1], level[entry[2] as keyof typeof level]]),
  );
}

describe("reusable release workflow permissions", () => {
  it("grants every notes job permission, including the skipped push redispatch job", () => {
    const caller = jobs(".github/workflows/create-release-branch.yml").get(
      "release-notes",
    );
    if (!caller) throw new Error("Release notes caller not found");
    expect(caller).toContain("uses: ./.github/workflows/release-notes.yml");
    const granted = permissions(caller);
    const called = jobs(".github/workflows/release-notes.yml");
    expect(called.has("redispatch")).toBe(true);
    for (const [name, job] of called) {
      const needed = permissions(job);
      expect(needed.size, name).toBeGreaterThan(0);
      for (const [permission, value] of needed) {
        expect(
          granted.get(permission) ?? 0,
          `${name}: ${permission}`,
        ).toBeGreaterThanOrEqual(value);
      }
    }
  });
});
