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
  const xmtpv3 = xmtp.getXMTPv3();
  const res = xmtpv3.selfTest();
  expect(res).toBe(true);
});

it("can do a simple conversation", async () => {
  const wasm = await XMTPWasm.initialize();
  const alice = wasm.newVoodooInstance();
  const bob = wasm.newVoodooInstance();

  const { sessionId, payload } = await alice.createOutboundSession(
    bob,
    "hello there"
  );
  expect(typeof sessionId).toBe("string");
  // Unused, but test JSON parseable
  const _ = JSON.parse(payload);
  const { payload: inboundPayload } = await bob.createInboundSession(
    alice,
    payload
  );
  expect(inboundPayload).toBe("hello there");
});

it("can send a message", async () => {
  const wasm = await XMTPWasm.initialize();
  const alice = wasm.newVoodooInstance();
  const bob = wasm.newVoodooInstance();

  const { sessionId, payload } = await alice.createOutboundSession(
    bob,
    "hello there"
  );
  await bob.createInboundSession(alice, payload);
  expect(typeof sessionId).toBe("string");

  const msg = "hello there";
  const encrypted = await alice.encryptMessage(sessionId, msg);
  expect(typeof encrypted).toBe("string");

  // Alice can't decrypt her own message. Does work for Bob though
  // const decrypted = await alice.decryptMessage(sessionId, encrypted);
  const decrypted = await bob.decryptMessage(sessionId, encrypted);
  expect(decrypted).toBe(msg);
});

it("can send a message to a public instance", async () => {
  const wasm = await XMTPWasm.initialize();
  const alice = wasm.newVoodooInstance();
  const bob = wasm.newVoodooInstance();

  const bobJson = bob.toPublicJSON();
  const publicBob = wasm.addOrGetPublicAccountFromJSON(bobJson);
  // Use publicBob to create the session
  const { sessionId, payload } = await alice.createOutboundSession(
    publicBob,
    "hello there"
  );
  await bob.createInboundSession(alice, payload);
  expect(typeof sessionId).toBe("string");

  // Test another round trip
  const msg = "hello there";
  const encrypted = await alice.encryptMessage(sessionId, msg);
  expect(typeof encrypted).toBe("string");

  const decrypted = await bob.decryptMessage(sessionId, encrypted);
  expect(decrypted).toBe(msg);

});
