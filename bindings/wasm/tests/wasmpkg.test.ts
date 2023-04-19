import { beforeEach, expect, it } from "vitest";
import { XmtpApi } from "..";

it("can instantiate", async () => {
  const a = await XmtpApi.initialize();
  expect(a).toBeDefined();

  // Make sure we can call it again
  const b = await XmtpApi.initialize();
  expect(b).toBeDefined();
});

let api: XmtpApi;

beforeEach(async () => {
  api = await XmtpApi.initialize();
});

it("can call into libxmtp", async () => {
  expect(api.addTwoNumbers(3, 4)).toBe(7);
})
