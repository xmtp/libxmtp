import assert from "node:assert/strict";
import { mkdtemp, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { validateTypeDoc } from "../scripts/typedoc-validation.mjs";

test("TypeDoc rejects a missing documentation link during validation", async () => {
  const root = await mkdtemp(join(tmpdir(), "xmtp-typedoc-test-"));
  try {
    const entry = join(root, "index.ts");
    const tsconfig = join(root, "tsconfig.json");
    await writeFile(
      tsconfig,
      JSON.stringify({
        compilerOptions: { types: [], skipLibCheck: true },
        files: [entry],
      }),
    );
    await writeFile(
      entry,
      '/** See {@link Missing}. */\nexport function example(): string { return "test"; }',
    );
    const options = {
      name: "Fixture",
      entryPoints: [entry],
      tsconfig,
      readme: "none",
    };
    await assert.rejects(
      validateTypeDoc(options, "Fixture"),
      /TypeDoc validation failed/,
    );
    await writeFile(
      entry,
      '/** Return the test value. */\nexport function example(): string { return "test"; }',
    );
    await validateTypeDoc(options, "Fixture");
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
