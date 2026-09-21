import { describe, expect, it, vi } from "vitest";
import { Client } from "@/Client";
import { createBackend } from "@/utils/createBackend";
import { createSigner } from "@test/helpers";

describe("Node backend authentication", () => {
  it("keeps credential sources separate for the same endpoint", async () => {
    const { signer } = createSigner();
    const identifier = await signer.getIdentifier();
    const credential = {
      value: "Bearer sdk-auth-test-key-00000000000000000000",
      expiresAtSeconds: Math.floor(Date.now() / 1000) + 3600,
    };
    const first = vi.fn(async () => credential);
    const second = vi.fn(async () => credential);
    const backendUrl = process.env.XMTP_BACKEND_URL!;
    const a = await createBackend({ backendUrl, authCallback: first });
    const b = await createBackend({ backendUrl, authCallback: second });
    await Client.canMessage([identifier], a);
    expect(first).toHaveBeenCalledOnce();
    expect(second).not.toHaveBeenCalled();
    await Client.canMessage([identifier], b);
    expect(first).toHaveBeenCalledOnce();
    expect(second).toHaveBeenCalledOnce();
  });
  it("uses the callback during client creation and identity reads", async () => {
    const { signer } = createSigner();
    const identifier = await signer.getIdentifier();
    const authCallback = vi.fn(async () => ({
      value: "Bearer sdk-auth-test-key-00000000000000000000",
      expiresAtSeconds: Math.floor(Date.now() / 1000) + 3600,
    }));
    await Client.fetchServerConfiguration({
      backendUrl: process.env.XMTP_BACKEND_URL!,
      authCallback,
    });
    expect(authCallback).not.toHaveBeenCalled();
    const client = await Client.build(identifier, {
      backendUrl: process.env.XMTP_BACKEND_URL!,
      dbPath: null,
      disableDeviceSync: true,
      authCallback,
    });
    try {
      expect(authCallback).toHaveBeenCalled();
      await expect(client.canMessage([identifier])).resolves.toBeInstanceOf(
        Map,
      );
    } finally {
      await client.close();
    }
  });

  it("sanitizes callback failures through native bindings", async () => {
    const { signer } = createSigner();
    const identifier = await signer.getIdentifier();
    const authCallback = vi.fn(() =>
      Promise.reject(new Error("private refresh response")),
    );
    await expect(
      Client.build(identifier, {
        backendUrl: process.env.XMTP_BACKEND_URL!,
        dbPath: null,
        authCallback,
      }),
    ).rejects.not.toThrow("private refresh response");
    expect(authCallback).toHaveBeenCalled();
  });

  it("refreshes a credential rejected by an auth-enabled backend", async ({
    skip,
  }) => {
    const backendUrl = process.env.XMTP_BACKEND_URL!;
    const configuration = await Client.fetchServerConfiguration(backendUrl);
    if (!configuration.auth.enabled) skip();
    const { signer } = createSigner();
    const identifier = await signer.getIdentifier();
    let calls = 0;
    const authCallback = vi.fn(async () => ({
      value:
        ++calls === 1
          ? "Bearer wrong-sdk-auth-key-00000000000000000000"
          : "Bearer sdk-auth-test-key-00000000000000000000",
      expiresAtSeconds: Math.floor(Date.now() / 1000) + 3600,
    }));
    await Client.canMessage([identifier], { backendUrl, authCallback });
    expect(authCallback).toHaveBeenCalledTimes(2);
  });
});
