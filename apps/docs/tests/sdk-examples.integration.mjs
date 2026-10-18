import assert from "node:assert/strict";
import test from "node:test";

import { ExpressiveCode } from "expressive-code";

import { examplePlugins } from "../scripts/example-config.mjs";

test("Twoslash resolves all local platform SDKs and renders all quickstart regions", async () => {
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
    "react-native": [["quickstart-react-native.ts", "app"]],
  };
  for (const [sdk, sources] of Object.entries(quickstarts)) {
    for (const [source, region] of sources) {
      const result = await engine.render({
        code: "",
        language: "ts",
        meta: `source="${source}" region="${region}"`,
      });
      const shown = result.renderedGroupContents[0].codeBlock.code;
      assert.ok(shown.length > 50);
      assert.doesNotMatch(shown, /#(?:end)?region|---cut-/u);
      assert.match(shown, new RegExp(`@xmtp/${sdk}-sdk`));
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
