import { createRegisteredClient, createSigner } from "@test/helpers";
import { Client } from "@xmtp/node-sdk";
import { describe, expect, it } from "vitest";

describe("inboxIdFor", () => {
  it("should return `undefined` inbox ID for unregistered address", async () => {
    const { identifier } = createSigner();
    const backend = { url: process.env.XMTP_BACKEND_URL! };
    const client = await createRegisteredClient(createSigner().signer);
    try {
      expect(await client.inboxIdFor(identifier)).toBeUndefined();
      expect(
        (await Client.canMessage([identifier], backend)).get(
          `ethereum:${identifier.identifier}`,
        ),
      ).toBe(false);
    } finally {
      await client.end();
    }
  });

  it("should return inbox ID for registered address", async () => {
    const { signer, identifier } = createSigner();
    const client = await createRegisteredClient(signer);
    const backend = { url: process.env.XMTP_BACKEND_URL! };
    const inboxId = await Client.inboxIdFor(identifier, backend);
    expect(inboxId).toBe(client.inboxId);
  });
});
