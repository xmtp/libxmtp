import { describe, expect, it, vi } from "vitest";

import type { AuthCallback, Credential } from "../src/types/options";
import { WorkerAuth, type AuthResponse } from "../src/utils/WorkerAuth";
import { WorkerBridge } from "../src/utils/WorkerBridge";

type Action = { action: "read"; id: string; data: undefined; result: number };
const credential = { value: "Bearer secret", expiresAtSeconds: 1234567890 };

const connect = (callback: AuthCallback, logging = false) => {
  const worker = {
    postMessage: vi.fn((response: AuthResponse) => {
      auth.receive(structuredClone(response));
    }),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    terminate: vi.fn(),
  };
  const bridge = new WorkerBridge<Action>(
    worker as unknown as Worker,
    logging,
    callback,
  );
  const auth = new WorkerAuth((request) => {
    bridge.handleMessage({ data: structuredClone(request) } as MessageEvent);
  });
  return { auth, bridge, worker };
};

describe("worker authentication", () => {
  it("returns credentials only to the requesting client without logging", async () => {
    const log = vi.spyOn(console, "log").mockImplementation(() => {});
    try {
      const first = connect(async () => credential, true);
      const secondCredential = { ...credential, value: "Bearer other" };
      const second = connect(async () => secondCredential, true);
      await expect(first.auth.request()).resolves.toEqual(credential);
      await expect(second.auth.request()).resolves.toEqual(secondCredential);
      expect(log).not.toHaveBeenCalled();
      first.auth.close();
      first.bridge.close();
      second.auth.close();
      second.bridge.close();
    } finally {
      log.mockRestore();
    }
  });

  it("sanitizes callback errors and never sends their contents", async () => {
    const { auth, bridge, worker } = connect(() =>
      Promise.reject(new Error("private token and refresh response")),
    );
    await expect(auth.request()).rejects.toThrow(/^auth callback failed$/);
    expect(worker.postMessage).toHaveBeenCalledWith({
      action: "auth.response",
      id: expect.any(String),
      failed: true,
    });
    auth.close();
    bridge.close();
  });

  it.each([NaN, Infinity, 0.5, Number.MAX_SAFE_INTEGER + 1])(
    "rejects an unsafe expiration (%s) before crossing the worker boundary",
    async (expiresAtSeconds) => {
      const { auth, bridge } = connect(async () => ({
        ...credential,
        expiresAtSeconds,
      }));
      await expect(auth.request()).rejects.toThrow(/^auth callback failed$/);
      auth.close();
      bridge.close();
    },
  );

  it("rejects pending worker requests on close and discards late credentials", async () => {
    let resolve!: (value: Credential) => void;
    const { auth, bridge, worker } = connect(
      () =>
        new Promise((complete) => {
          resolve = complete;
        }),
    );
    const pending = auth.request();
    const rejected = expect(pending).rejects.toThrow("auth callback failed");
    auth.close();
    bridge.close();
    await rejected;
    resolve(credential);
    await Promise.resolve();
    await Promise.resolve();
    expect(worker.postMessage).not.toHaveBeenCalled();
    await expect(auth.request()).rejects.toThrow("auth callback failed");
  });

  it("rejects a request if the transport cannot send it", async () => {
    const auth = new WorkerAuth(() => {
      throw new Error("transport secret");
    });
    await expect(auth.request()).rejects.toThrow(/^auth callback failed$/);
    auth.close();
  });
});
