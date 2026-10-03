import { generateInboxId as rootGenerateInboxId } from "@xmtp/browser-sdk";
import { generateInboxId, initPureWasm } from "@xmtp/browser-sdk/pure";
import { expect, test } from "vitest";

test("the public root uses the same synchronous pure inbox calculation", async () => {
  await initPureWasm();
  expect(rootGenerateInboxId).toBe(generateInboxId);
  const actual = rootGenerateInboxId(
    {
      kind: "ethereum",
      identifier: "0xabcdef0000000000000000000000000000000000",
    },
    9_007_199_254_740_993n,
  );
  expect(actual).toBeTypeOf("string");
  expect(actual).toBe(
    "7388e86684247cde39d20ef985b6999cb657325d1576c28b74670a5392228913",
  );
});
