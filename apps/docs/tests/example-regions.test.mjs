import assert from "node:assert/strict";
import { mkdtemp, writeFile, symlink } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { ExpressiveCode } from "expressive-code";
import twoslash from "expressive-code-twoslash";
import ts from "typescript";
import {
  exampleRegions,
  extractRegion,
  readRegion,
  validateExample,
} from "../scripts/example-regions.mjs";

const source =
  "const prefix = 3;\n// #region value\nconst value = prefix + 1;\n// #endregion value\nconsole.log(value);";

test("a region keeps hidden context and excludes markers", () => {
  const region = extractRegion(source, "value");
  assert.equal(region.code, "const value = prefix + 1;");
  assert.match(region.program, /const prefix = 3;\n\/\/ ---cut-before---/u);
  assert.match(region.program, /\/\/ ---cut-after---\nconsole.log\(value\);/u);
  assert.doesNotMatch(region.program, /#(?:end)?region/u);
});

test("malformed region boundaries fail closed", () => {
  assert.throws(() => extractRegion(source, "missing"), /Missing region/u);
  assert.throws(
    () => extractRegion(source + "\n" + source, "value"),
    /Duplicate region/u,
  );
  assert.throws(
    () => extractRegion("// #region x\n// #region y", "x"),
    /Nested region/u,
  );
  assert.throws(
    () => extractRegion("// #endregion x", "x"),
    /Unmatched region end/u,
  );
  assert.throws(
    () => extractRegion("// #region x\n1", "x"),
    /Unclosed region/u,
  );
  assert.throws(
    () => extractRegion("// #region x\n// #endregion x", "x"),
    /Empty region/u,
  );
});

test("examples cannot suppress checking or substitute any", () => {
  assert.throws(
    () => validateExample("// @ts-nocheck\nconst a = 1;"),
    /must not suppress/u,
  );
  assert.throws(
    () => validateExample("// @noErrors\nconst a = 1;"),
    /must not suppress/u,
  );
  assert.throws(
    () => validateExample("const a: any = 1;"),
    /must not use the any/u,
  );
});

test("source reads reject files and symlinks outside examples", async () => {
  const root = await mkdtemp(join(tmpdir(), "xmtp-example-root-"));
  const outside = await mkdtemp(join(tmpdir(), "xmtp-example-outside-"));
  const external = join(outside, "private.ts");
  await writeFile(external, source);
  await symlink(external, join(root, "linked.ts"));
  await assert.rejects(readRegion(external, "value", root), /inside examples/u);
  await assert.rejects(
    readRegion("linked.ts", "value", root),
    /inside examples/u,
  );
  await assert.rejects(readRegion("missing.ts", "value", root), /ENOENT/u);
});

test("source fences render the selected compiled region and reject bad input", async () => {
  const root = await mkdtemp(join(tmpdir(), "xmtp-example-render-"));
  await writeFile(join(root, "valid.ts"), source);
  await writeFile(
    join(root, "invalid.ts"),
    source.replace("prefix + 1", "prefix.noSuchMethod()"),
  );
  const engine = new ExpressiveCode({
    logger: { error: () => {} },
    plugins: [
      exampleRegions({ root }),
      twoslash({
        twoslashOptions: {
          compilerOptions: {
            strict: true,
            types: [],
            target: ts.ScriptTarget.ESNext,
            module: ts.ModuleKind.ESNext,
          },
        },
      }),
    ],
  });
  const result = await engine.render({
    code: "",
    language: "ts",
    meta: 'source="valid.ts" region="value"',
  });
  assert.equal(
    result.renderedGroupContents[0].codeBlock.code.trimEnd(),
    "const value = prefix + 1;",
  );
  await assert.rejects(
    engine.render({
      code: "",
      language: "ts",
      meta: 'source="invalid.ts" region="value"',
    }),
    /noSuchMethod/u,
  );
  await assert.rejects(
    engine.render({
      code: "",
      language: "ts",
      meta: 'source="valid.ts" region="missing"',
    }),
    /Missing region/u,
  );
  await assert.rejects(
    engine.render({ code: "", language: "ts", meta: 'source="valid.ts"' }),
    /one source and one region/u,
  );
  await assert.rejects(
    engine.render({
      code: "const stale = 1;",
      language: "ts",
      meta: 'source="valid.ts" region="value"',
    }),
    /must be empty/u,
  );
  await assert.rejects(
    engine.render({ code: "const unchecked = 1;", language: "ts" }),
    /need source regions or twoslash/u,
  );
  const reference = await engine.render({
    code: "function declarationOnly(): string;",
    language: "ts",
    parentDocument: { sourceFilePath: "/docs/reference/node-sdk/Client.md" },
  });
  assert.match(
    reference.renderedGroupContents[0].codeBlock.code,
    /declarationOnly/u,
  );
});
