import { describe, expect, it, vi } from "vitest";
import { WorkerBridge } from "../src/utils/WorkerBridge";

type Action = { action: "read"; id: string; data: undefined; result: number };

const createBridge = () => {
  const worker = {
    postMessage: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    terminate: vi.fn(),
  };
  return {
    worker,
    bridge: new WorkerBridge<Action>(worker as unknown as Worker),
  };
};

describe("WorkerBridge close", () => {
  it("fences new actions before the worker finishes closing", async () => {
    const { worker, bridge } = createBridge();
    let finish: (() => void) | undefined;
    const closing = bridge.closeAfter(
      new Promise<void>((resolve) => {
        finish = resolve;
      }),
    );
    expect(bridge.isClosed).toBe(true);
    expect(() => bridge.action("read")).toThrow("closed");
    expect(worker.terminate).not.toHaveBeenCalled();
    finish?.();
    await closing;
    expect(worker.terminate).toHaveBeenCalledOnce();
  });

  it("rejects pending actions when the worker stops", async () => {
    const { bridge } = createBridge();
    const read = bridge.action("read");
    const rejected = expect(read).rejects.toThrow("closed");
    bridge.close();
    await rejected;
  });

  it("detaches legacy listeners and rejects their queued callbacks at close", async () => {
    const { worker, bridge } = createBridge();
    const onValue = vi.fn();
    const onFail = vi.fn();
    const dispose = bridge.handleStreamMessage("stream", onValue, { onFail });
    const queued = worker.addEventListener.mock.calls.at(-1)?.[1] as (
      event: MessageEvent,
    ) => void;
    let finish: (() => void) | undefined;
    const closing = bridge.closeAfter(
      new Promise<void>((resolve) => {
        finish = resolve;
      }),
    );
    expect(worker.removeEventListener).toHaveBeenCalledWith("message", queued);
    queued({
      data: { action: "stream.preferences", streamId: "stream", result: [] },
    } as MessageEvent);
    queued({
      data: { action: "stream.fail", streamId: "stream", result: undefined },
    } as MessageEvent);
    expect(onValue).not.toHaveBeenCalled();
    expect(onFail).not.toHaveBeenCalled();
    await expect(dispose()).resolves.toBeUndefined();
    expect(worker.postMessage).not.toHaveBeenCalled();
    finish?.();
    await closing;
  });

  it("terminates the worker if core close fails", async () => {
    const { worker, bridge } = createBridge();
    await expect(
      bridge.closeAfter(Promise.reject(new Error("close failed"))),
    ).rejects.toThrow("close failed");
    expect(worker.terminate).toHaveBeenCalledOnce();
  });

  it("rejects pending reads after a terminal worker error", async () => {
    const { worker, bridge } = createBridge();
    const logged = vi.spyOn(console, "error").mockImplementation(() => {});
    try {
      const read = bridge.action("read");
      const rejected = expect(read).rejects.toThrow("worker stopped");
      bridge.handleError({ message: "worker stopped" } as ErrorEvent);
      await rejected;
      expect(bridge.isClosed).toBe(true);
      expect(worker.terminate).toHaveBeenCalledOnce();
    } finally {
      logged.mockRestore();
    }
  });
});
