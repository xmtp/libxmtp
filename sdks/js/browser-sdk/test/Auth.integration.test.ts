import { describe, expect, it, vi } from "vitest";
import { Client } from "@/Client";
import { createBackend } from "@/utils/createBackend";
import { createSigner } from "@test/helpers";

describe("Browser worker authentication", () => {
  it("keeps standalone credential sources separate for the same endpoint", async () => {
    const { identifier } = createSigner();
    const credential = {
      value: "Bearer sdk-auth-test-key-00000000000000000000",
      expiresAtSeconds: Math.floor(Date.now() / 1000) + 3600,
    };
    const first = vi.fn(async () => credential);
    const second = vi.fn(async () => credential);
    const backendUrl = import.meta.env.XMTP_BACKEND_URL;
    const a = await createBackend({ backendUrl, authCallback: first });
    const b = await createBackend({ backendUrl, authCallback: second });
    try {
      await Client.canMessage([identifier], a);
      expect(first).toHaveBeenCalledOnce();
      expect(second).not.toHaveBeenCalled();
      await Client.canMessage([identifier], b);
      expect(first).toHaveBeenCalledOnce();
      expect(second).toHaveBeenCalledOnce();
      await expect(Client.build(identifier, { backend: a })).rejects.toThrow(
        "cannot be transferred to the worker",
      );
    } finally {
      a.free();
      b.free();
    }
  });
  it("uses the app callback during client creation and identity reads", async () => {
    const { identifier } = createSigner();
    const authCallback = vi.fn(async () => ({
      value: "Bearer sdk-auth-test-key-00000000000000000000",
      expiresAtSeconds: Math.floor(Date.now() / 1000) + 3600,
    }));
    await Client.fetchServerConfiguration({
      backendUrl: import.meta.env.XMTP_BACKEND_URL,
      authCallback,
    });
    expect(authCallback).not.toHaveBeenCalled();
    const client = await Client.build(identifier, {
      backendUrl: import.meta.env.XMTP_BACKEND_URL,
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

  it("returns a sanitized callback failure through the real worker", async () => {
    const { identifier } = createSigner();
    const authCallback = vi.fn(() =>
      Promise.reject(new Error("private refresh response")),
    );
    const creation = Client.build(identifier, {
      backendUrl: import.meta.env.XMTP_BACKEND_URL,
      dbPath: null,
      authCallback,
    });
    await expect(creation).rejects.not.toThrow("private refresh response");
    expect(authCallback).toHaveBeenCalled();
  });

  it("refreshes a rejected credential through the worker", async ({ skip }) => {
    const backendUrl = import.meta.env.XMTP_BACKEND_URL;
    const configuration = await Client.fetchServerConfiguration(backendUrl);
    if (!configuration.auth.enabled) skip();
    const { identifier } = createSigner();
    let calls = 0;
    const authCallback = vi.fn(async () => ({
      value:
        ++calls === 1
          ? "Bearer wrong-sdk-auth-key-00000000000000000000"
          : "Bearer sdk-auth-test-key-00000000000000000000",
      expiresAtSeconds: Math.floor(Date.now() / 1000) + 3600,
    }));
    const client = await Client.build(identifier, {
      backendUrl,
      authCallback,
      dbPath: null,
      disableDeviceSync: true,
    });
    try {
      expect(authCallback).toHaveBeenCalledTimes(2);
    } finally {
      await client.close();
    }
  });
});
