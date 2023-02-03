import { expect, it } from "vitest";
 import { XmtpApi } from "..";

it("can instantiate", async () => {
  const a = await XmtpApi.initialize();
  expect(a).toBeDefined();

  // Make sure we can call it again
  const b = await XmtpApi.initialize();
  expect(b).toBeDefined();
});

it("can generate mnemonic", async () => {
  // Make sure we can call it again
  const b = await XmtpApi.initialize();
  expect(b).toBeDefined();
  // Generate a key
  const key = await b.generateMnemonic();
  // Make sure it splits to 12 words
  expect(key.split(" ").length).toBe(12);
});
