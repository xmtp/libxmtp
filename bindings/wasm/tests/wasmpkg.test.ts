import { expect, it } from "vitest";
import { XMTPWasm } from "..";

it("can instantiate", async () => {
  const a = await XMTPWasm.initialize();
  expect(a).toBeDefined();

  // Make sure we can call it again
  const b = await XMTPWasm.initialize();
  expect(b).toBeDefined();
});

it("can run self test", async () => {
  const xmtp = await XMTPWasm.initialize();
  const xmtpv3 = await xmtp.getXMTPv3();
  const res = await xmtpv3.selfTest();
  expect(res).toBe(true);
});
