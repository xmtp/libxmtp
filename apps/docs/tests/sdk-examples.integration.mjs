import assert from "node:assert/strict";
import test from "node:test";
import { ExpressiveCode } from "expressive-code";
import { examplePlugins } from "../scripts/example-config.mjs";

test("Twoslash resolves both local SDKs and renders all quickstart regions", async () => {
  const engine = new ExpressiveCode({
    plugins: examplePlugins(),
    logger: { error: () => {} },
  });
  for (const sdk of ["node", "browser"]) {
    for (const region of ["client", "send", "stream"]) {
      const result = await engine.render({
        code: "",
        language: "ts",
        meta: `source="quickstart-${sdk}.ts" region="${region}"`,
      });
      const shown = result.renderedGroupContents[0].codeBlock.code;
      assert.ok(shown.length > 50);
      assert.doesNotMatch(shown, /#(?:end)?region|---cut-/u);
      if (region === "client")
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
