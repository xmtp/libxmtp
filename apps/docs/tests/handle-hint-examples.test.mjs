import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import test from "node:test";

import ts from "typescript";

import { readRegion } from "../scripts/example-regions.mjs";

// U+212A KELVIN SIGN: toLowerCase() folds it to ASCII "k", but the SDK
// lowercases ASCII capitals only and refuses it
const KELVIN = "K";

const loadHint = async (filename) => {
  const { code } = await readRegion(filename, "hint");
  const { outputText } = ts.transpileModule(code, {
    compilerOptions: {
      module: ts.ModuleKind.ESNext,
      target: ts.ScriptTarget.ES2022,
    },
  });
  const module = await import(
    `data:text/javascript,${encodeURIComponent(outputText)}`
  );
  return module.handleHint;
};

const client = { serverConfiguration: { handles: { minNameLength: 3 } } };

for (const filename of ["inboxes-node.ts", "inboxes-browser.ts"]) {
  test(`${filename} hint lowercases ASCII capitals only`, async () => {
    const handleHint = await loadHint(filename);
    assert.equal(handleHint(client, "Nick#2341"), undefined);
    assert.equal(
      handleHint(client, `${KELVIN}ate`),
      "Start with a letter. Use letters, digits, _ and -.",
    );
  });
}

test("the Kotlin and Swift hints lowercase ASCII capitals only", async () => {
  const page = await readFile(
    resolve(import.meta.dirname, "../src/content/docs/sdk/inboxes.mdx"),
    "utf8",
  );
  const blocks = [...page.matchAll(/```(kotlin|swift)\n([\s\S]*?)```/gu)]
    .filter(([, , body]) => body.includes("handleHint"))
    .map(([, language, body]) => ({ language, body }));
  assert.deepEqual(
    blocks.map(({ language }) => language),
    ["kotlin", "swift"],
  );
  for (const { language, body } of blocks) {
    // Kotlin lowercase() and Swift lowercased() fold U+212A to "k"
    assert.doesNotMatch(
      body,
      /typed\.lowercased?\(\)/u,
      `${language} hint folds non-ASCII letters`,
    );
  }
});
