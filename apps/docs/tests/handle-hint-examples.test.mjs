import assert from "node:assert/strict";
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
