import assert from "node:assert/strict";
import { mkdtemp, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { Application, Converter, Logger } from "typedoc";

import { typeDocValidation } from "../scripts/typedoc-validation.mjs";

test("TypeDoc validates one conversion and retains bootstrap diagnostics", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "xmtp-typedoc-test-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const entry = join(root, "index.ts");
  const tsconfig = join(root, "tsconfig.json");
  await writeFile(
    tsconfig,
    JSON.stringify({
      compilerOptions: { types: [], skipLibCheck: true },
      files: [entry],
    }),
  );
  for (const [comment, options, fails] of [
    ["Return the test value.", {}, false],
    ["See {@link Missing}.", {}, true],
    ["Return the test value.", { unknownFixtureOption: true }, true],
  ]) {
    await writeFile(
      entry,
      `/** ${comment} */\nexport function example(): string { return "test"; }`,
    );
    const app = await Application.bootstrapWithPlugins({
      name: "Fixture",
      entryPoints: [entry],
      tsconfig,
      readme: "none",
      plugin: [typeDocValidation("Fixture")],
      ...options,
    });
    let conversions = 0;
    app.converter.on(Converter.EVENT_BEGIN, () => conversions++);
    // Match Starlight's logger replacement after bootstrap.
    app.logger = new Logger();
    if (fails) {
      await assert.rejects(app.convert(), /Fixture TypeDoc validation failed/);
    } else {
      assert.equal((await app.convert()).children[0].name, "example");
    }
    assert.equal(conversions, 1);
  }
});
