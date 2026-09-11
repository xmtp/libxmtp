import { IdentifierKind } from "@xmtp/wasm-bindings";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Client } from "@/Client";
import type { Signer } from "@/utils/signer";

const identifier = {
  identifier: "0x0000000000000000000000000000000000000001",
  identifierKind: IdentifierKind.Ethereum,
};
const options = { backendUrl: "http://unused.invalid", dbPath: null };
const failures = new Map<string, Error>();

class TestWorker {
  static instance: TestWorker;
  listeners = new Map<string, (event: MessageEvent | ErrorEvent) => void>();
  terminate = vi.fn();
  postMessage = vi.fn((request: { action: string; id: string }) => {
    queueMicrotask(() => {
      const error = failures.get(request.action);
      this.listeners.get("message")?.({
        data: {
          id: request.id,
          action: request.action,
          ...(error
            ? { error }
            : {
                result: {
                  identifier,
                  inboxId: "inbox",
                  installationId: "installation",
                  installationIdBytes: new Uint8Array(),
                  appVersion: "",
                  env: "test",
                  libxmtpVersion: "test",
                },
              }),
        },
      } as MessageEvent);
    });
  });

  constructor() {
    TestWorker.instance = this;
  }

  addEventListener(
    type: string,
    listener: (event: MessageEvent | ErrorEvent) => void,
  ) {
    this.listeners.set(type, listener);
  }

  removeEventListener(type: string) {
    this.listeners.delete(type);
  }
}

describe("Client close after failure", () => {
  beforeEach(() => {
    failures.clear();
    vi.stubGlobal("Worker", TestWorker);
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("closes a ready client after the worker has already stopped", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    const client = await Client.build(identifier, { ...options });
    const worker = TestWorker.instance;
    expect(client.isReady).toBe(true);
    worker.listeners.get("error")?.({
      message: "worker stopped",
      error: new Error("worker stopped"),
    } as ErrorEvent);

    const closed = client.close();
    expect(client.close()).toBe(closed);
    await expect(closed).resolves.toBeUndefined();
    expect(client.isReady).toBe(false);
    expect(worker.postMessage).toHaveBeenCalledOnce();
    expect(worker.terminate).toHaveBeenCalledOnce();
  });

  it.each(["create", "build"] as const)(
    "keeps the %s error if cleanup also rejects",
    async (method) => {
      const initialError = new Error("initialization failed");
      failures.set("client.init", initialError);
      vi.spyOn(Client.prototype, "close").mockRejectedValue(
        new Error("cleanup failed"),
      );
      const signer: Signer = {
        type: "EOA",
        getIdentifier: () => identifier,
        signMessage: () => new Uint8Array(),
      };
      const result =
        method === "create"
          ? Client.create(signer, { ...options })
          : Client.build(identifier, { ...options });
      await expect(result).rejects.toBe(initialError);
    },
  );

  it("keeps the registration error when core close also fails", async () => {
    const initialError = new Error("registration failed");
    failures.set("client.createInboxSignatureText", initialError);
    failures.set("client.close", new Error("core close failed"));
    const signer: Signer = {
      type: "EOA",
      getIdentifier: () => identifier,
      signMessage: () => new Uint8Array(),
    };

    await expect(Client.create(signer, { ...options })).rejects.toBe(
      initialError,
    );
    expect(TestWorker.instance.terminate).toHaveBeenCalledOnce();
  });

  it("terminates the worker when the close action cannot be sent", async () => {
    const client = await Client.build(identifier, { ...options });
    const worker = TestWorker.instance;
    const failure = new Error("postMessage failed");
    worker.postMessage.mockImplementationOnce(() => {
      throw failure;
    });

    const closed = client.close();
    expect(client.close()).toBe(closed);
    await expect(closed).rejects.toBe(failure);
    expect(client.isReady).toBe(false);
    expect(worker.terminate).toHaveBeenCalledOnce();
  });
});
