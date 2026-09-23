import type { Conversation } from "@xmtp/wasm-bindings";
import { describe, expect, it, vi } from "vitest";

import type { WorkerClient } from "@/WorkerClient";
import { WorkerConversation } from "@/WorkerConversation";

describe("WorkerConversation app data", () => {
  const client = {} as WorkerClient;

  it("keeps the empty value returned for a group without app data", () => {
    const native = { appData: vi.fn(() => "") } as unknown as Conversation;

    expect(new WorkerConversation(client, native).appData).toBe("");
  });

  it("preserves a native storage error", () => {
    const storageError = new Error("[GroupError::Storage] database is full");
    const native = {
      appData: vi.fn(() => {
        throw storageError;
      }),
    } as unknown as Conversation;

    expect(() => new WorkerConversation(client, native).appData).toThrow(
      storageError,
    );
  });
});
