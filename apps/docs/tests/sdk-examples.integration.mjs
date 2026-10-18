import assert from "node:assert/strict";
import { realpathSync } from "node:fs";
import { resolve } from "node:path";
import test from "node:test";

import { ExpressiveCode } from "expressive-code";
import ts from "typescript";

import {
  docsRoot,
  exampleConfig,
  examplePlugins,
} from "../scripts/example-config.mjs";
import { sdkEntry } from "../scripts/sdk-entry.mjs";

test("TypeDoc declarations match public SDK package resolution", () => {
  const { options } = exampleConfig();
  const importer = resolve(docsRoot, "examples/package-entry.ts");
  for (const [sdk, suffix] of [
    ["node", ""],
    ["browser", ""],
    ["agent", ""],
    ["react-native", ""],
    ["react-native", "/react"],
  ]) {
    const packageRoot = resolve(docsRoot, "../../sdks", sdk);
    const specifier = `@xmtp/${sdk}-sdk${suffix}`;
    const resolved = ts.resolveModuleName(specifier, importer, options, ts.sys);
    assert.ok(resolved.resolvedModule, `${specifier} must resolve`);
    assert.equal(
      realpathSync(resolved.resolvedModule.resolvedFileName),
      realpathSync(sdkEntry(packageRoot, `.${suffix}`)),
    );
    assert.throws(
      () => sdkEntry(packageRoot, "./private"),
      /export is missing/,
    );
    assert.equal(
      ts.resolveModuleName(`${specifier}/private`, importer, options, ts.sys)
        .resolvedModule,
      undefined,
    );
  }
});

test("Twoslash checks platform SDK APIs and renders example regions", async () => {
  const engine = new ExpressiveCode({
    plugins: examplePlugins(),
    logger: { error: () => {} },
  });
  const quickstarts = {
    node: [
      ["quickstart-node-listen.ts", "listen"],
      ["quickstart-node-send.ts", "send"],
    ],
    browser: [["quickstart-browser.ts", "app"]],
    "react-native": [
      ["quickstart-react-native.ts", "app"],
      ["react-native-react.ts", "initialize", "useClient()"],
    ],
  };
  for (const [sdk, sources] of Object.entries(quickstarts)) {
    for (const [source, region, expected = `@xmtp/${sdk}-sdk`] of sources) {
      const result = await engine.render({
        code: "",
        language: "ts",
        meta: `source="${source}" region="${region}"`,
      });
      const shown = result.renderedGroupContents[0].codeBlock.code;
      assert.ok(shown.length > 50);
      assert.doesNotMatch(shown, /#(?:end)?region|---cut-/u);
      assert.ok(shown.includes(expected));
    }
    await assert.rejects(
      engine.render({
        language: "ts",
        meta: "twoslash",
        code: `import { Client } from "@xmtp/${sdk}-sdk";\nClient.methodThatDoesNotExist();`,
      }),
      /methodThatDoesNotExist/u,
    );
  }
});
