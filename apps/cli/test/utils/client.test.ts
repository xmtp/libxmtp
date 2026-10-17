import { Client, initLogging, type BackendSource } from "@xmtp/node-sdk";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { createClient } from "@/utils/client";

vi.mock("@xmtp/node-sdk", () => ({
  Client: { create: vi.fn(async () => ({})) },
  initLogging: vi.fn(async () => undefined),
}));

beforeEach(() => vi.clearAllMocks());

describe("CLI logging config", () => {
  it("starts structured logging without an explicit log level", async () => {
    await createClient(
      {
        walletKey: "11".repeat(32),
        dbEncryptionKey: "22".repeat(32),
        structuredLogging: true,
      },
      { url: "https://example.com" } as BackendSource,
    );
    expect(initLogging).toHaveBeenCalledWith({
      level: undefined,
      structured: true,
    });
    expect(Client.create).toHaveBeenCalledOnce();
  });
});
