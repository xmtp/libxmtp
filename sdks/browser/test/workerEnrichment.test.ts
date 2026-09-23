import { afterEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ create: vi.fn() }));
vi.mock("@xmtp/wasm-bindings", () => ({
  default: vi.fn(async () => {}),
  LogLevel: { Off: 0 },
}));
vi.mock("../src/WorkerClient", () => ({
  WorkerClient: { create: mocks.create },
}));
vi.mock("../src/WorkerConversation", () => ({ WorkerConversation: class {} }));
vi.mock("../src/utils/conversions", () => ({ toSafeConversation: vi.fn() }));

afterEach(() => {
  vi.unstubAllGlobals();
  vi.resetModules();
});

describe("worker strict delivery enrichment", () => {
  it("uses a fresh reader for each enrichment outcome", async () => {
    const worker = {
      postMessage: vi.fn(),
      onmessage: undefined as
        | undefined
        | ((event: { data: unknown }) => Promise<void>),
    };
    vi.stubGlobal("self", worker);
    await import("../src/workers/client");
    const send = (action: string, data: unknown) =>
      worker.onmessage!({ data: { id: action, action, data } });

    for (const outcome of ["storage", "selection", "success"] as const) {
      const cause = new Error(
        "[StorageError::Connection] database unavailable",
      );
      const message = { id: "retained" };
      const acknowledgement = {
        enrichedMessage: vi.fn(() => {
          if (outcome === "storage") throw cause;
          return outcome === "selection" ? null : message;
        }),
        free: vi.fn(),
        reject: vi.fn(),
      };
      const cursor = { databaseId: new Uint8Array(16), deliverySequence: 1n };
      const item = { acknowledgement, cursor, message, free: vi.fn() };
      const reader = {
        nextDelivery: vi.fn(async () => item),
        close: vi.fn(),
      };
      const lookup = vi.fn(async () => undefined);
      mocks.create.mockResolvedValue({
        conversations: { messageReader: () => reader, getMessageById: lookup },
        close: vi.fn(async () => {}),
      });
      await send("client.init", { identifier: {} });
      await send("messageReader.open", { readerId: "reader" });
      await send("messageReader.next", { readerId: "reader" });
      const response = worker.postMessage.mock.lastCall?.[0];
      if (outcome === "storage") {
        expect(response.error).toBe(cause);
        expect(response.result).toBeUndefined();
      } else {
        expect(response.result.message).toBe(
          outcome === "selection" ? undefined : message,
        );
        expect(response.result.cursor).toBe(cursor);
      }
      expect(acknowledgement.enrichedMessage).toHaveBeenCalledOnce();
      expect(lookup).not.toHaveBeenCalled();
      expect(item.free).toHaveBeenCalledOnce();
      await send("messageReader.close", { readerId: "reader" });
      expect(reader.close).toHaveBeenCalledOnce();
      expect(acknowledgement.free).toHaveBeenCalledOnce();
      await send("client.close", undefined);
      worker.postMessage.mockClear();
    }
  });
});
