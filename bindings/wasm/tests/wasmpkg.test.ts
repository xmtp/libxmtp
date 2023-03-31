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

it("can do a simple conversation", async () => {
  const wasm = await XMTPWasm.initialize();
  const alice = await wasm.newVoodooInstance();
  const bob = await wasm.newVoodooInstance();

  const outboundJson = await alice.createOutboundSession(bob.handle, "hello there");
  // Unused, but test JSON parseable
  const outbound = JSON.parse(outboundJson);
  const inboundJson = await bob.createInboundSession(alice.handle, outboundJson);
  const inbound = JSON.parse(inboundJson);
  console.log("outbound", outbound);
  console.log("inbound", inbound);
  // This inbound should have a plaintext field with "hello there"
  expect(inbound.plaintext).toBe("hello there");
});
