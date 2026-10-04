import { createSigner } from "@test/helpers";
import { createRegisteredClient, clientOptions } from "@test/helpers";
import { Backend, Client } from "@xmtp/node-sdk";
import { describe, expect, it, vi } from "vitest";

describe("Node backend authentication", () => {
  it("keeps credential sources separate for the same endpoint", async () => {
    const { signer } = createSigner();
    const identifier = await signer.identity();
    const credential = {
      value: "Bearer sdk-auth-test-key-00000000000000000000",
      expiresAtSeconds: BigInt(Math.floor(Date.now() / 1000) + 3600),
    };
    const first = vi.fn(async () => credential);
    const second = vi.fn(async () => credential);
    const backendUrl = process.env.XMTP_BACKEND_URL!;
    const a = await Backend.connect({
      url: backendUrl,
      credentials: { credential: first },
    });
    const b = await Backend.connect({
      url: backendUrl,
      credentials: { credential: second },
    });
    await Client.canMessage([identifier], a);
    expect(first).toHaveBeenCalledOnce();
    expect(second).not.toHaveBeenCalled();
    await Client.canMessage([identifier], b);
    expect(first).toHaveBeenCalledOnce();
    expect(second).toHaveBeenCalledOnce();
  });
  it("uses the callback during client creation and identity reads", async () => {
    const { signer } = createSigner();
    const identifier = await signer.identity();
    const authCallback = vi.fn(async () => ({
      value: "Bearer sdk-auth-test-key-00000000000000000000",
      expiresAtSeconds: BigInt(Math.floor(Date.now() / 1000) + 3600),
    }));
    await Client.fetchServerConfiguration({
      url: process.env.XMTP_BACKEND_URL!,
      credentials: { credential: authCallback },
    });
    expect(authCallback).not.toHaveBeenCalled();
    const options = clientOptions({
      backend: {
        url: process.env.XMTP_BACKEND_URL!,
        credentials: { credential: authCallback },
      },
    });
    const stored = await createRegisteredClient(signer, options);
    await stored.end();
    const client = await Client.build(identifier, options);
    try {
      expect(authCallback).toHaveBeenCalled();
      await expect(client.canMessage([identifier])).resolves.toBeInstanceOf(
        Map,
      );
    } finally {
      await client.end();
    }
  });

  it("sanitizes callback failures through native bindings", async () => {
    const { signer } = createSigner();
    const authCallback = vi.fn(() =>
      Promise.reject(new Error("private refresh response")),
    );
    await expect(
      Client.create(
        signer,
        clientOptions({
          backend: {
            url: process.env.XMTP_BACKEND_URL!,
            credentials: { credential: authCallback },
          },
        }),
      ),
    ).rejects.not.toThrow("private refresh response");
    expect(authCallback).toHaveBeenCalled();
  });

  it("refreshes a credential rejected by an auth-enabled backend", async ({
    skip,
  }) => {
    const backendUrl = process.env.XMTP_BACKEND_URL!;
    const configuration = await Client.fetchServerConfiguration({
      url: backendUrl,
    });
    if (!configuration.auth.enabled) skip();
    const { signer } = createSigner();
    const identifier = await signer.identity();
    let calls = 0;
    const authCallback = vi.fn(async () => ({
      value:
        ++calls === 1
          ? "Bearer wrong-sdk-auth-key-00000000000000000000"
          : "Bearer sdk-auth-test-key-00000000000000000000",
      expiresAtSeconds: BigInt(Math.floor(Date.now() / 1000) + 3600),
    }));
    await Client.canMessage([identifier], {
      url: backendUrl,
      credentials: { credential: authCallback },
    });
    expect(authCallback).toHaveBeenCalledTimes(2);
  });
});
