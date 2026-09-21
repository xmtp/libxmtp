import { Client } from "@xmtp/node-sdk";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Agent } from "./Agent";
import { createSigner, createUser } from "@/user/User";

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllEnvs();
});

describe("agent backend authentication", () => {
  it.each(["create", "createFromEnv"] as const)(
    "forwards authCallback through %s without invoking it",
    async (method) => {
      vi.stubEnv("XMTP_DB_DIRECTORY", undefined);
      vi.stubEnv("XMTP_BACKEND_URL", "https://backend.example.com");
      const key = `0x${"01".repeat(32)}` as const;
      vi.stubEnv("XMTP_WALLET_KEY", key);
      const stopped = new Error("client creation reached");
      const create = vi.spyOn(Client, "create").mockRejectedValue(stopped);
      const authCallback = vi.fn(async () => ({
        value: "Bearer secret",
        expiresAtSeconds: 1234567890,
      }));
      const options = {
        backendUrl: "https://backend.example.com",
        authCallback,
      };
      await expect(
        method === "create"
          ? Agent.create(createSigner(createUser(key)), options)
          : Agent.createFromEnv(options),
      ).rejects.toBe(stopped);
      expect(create).toHaveBeenCalledWith(
        expect.anything(),
        expect.objectContaining(options),
      );
      expect(authCallback).not.toHaveBeenCalled();
    },
  );
});
