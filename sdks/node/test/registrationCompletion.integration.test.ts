import { createClient, createSigner } from "@test/helpers";
import { expect, it } from "vitest";

it("completes registration and keeps repeated registration idempotent on the same client", async () => {
  const { signer } = createSigner();
  let signCalls = 0;
  const client = await createClient({
    ...signer,
    sign: async (request) => {
      signCalls += 1;
      return signer.sign(request);
    },
  });
  try {
    expect(await client.isRegistered()).toBe(false);
    await expect(client.register()).resolves.toBeUndefined();
    expect(await client.isRegistered()).toBe(true);
    const signed = signCalls;
    expect(signed).toBeGreaterThan(0);
    await expect(client.register()).resolves.toBeUndefined();
    await expect(client.register()).resolves.toBeUndefined();
    expect(await client.isRegistered()).toBe(true);
    expect(signCalls).toBe(signed);
  } finally {
    await client.end();
  }
});
