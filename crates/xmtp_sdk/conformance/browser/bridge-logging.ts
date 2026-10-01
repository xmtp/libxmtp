import { expect, it, vi } from "vitest";

import { MainCallbacks } from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/main/callbacks.js";
import { WorkerCallbacks } from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/worker/callback-stub.js";
import { pair } from "./bridge-support.js";

export function registerLoggingTests(): void {
  // verifies: LOG-007, LOG-011
  it("log admission rejects a packet after replace or clear", async () => {
    const [main] = pair();
    const callbacks = new MainCallbacks(main);
    const old = vi.fn();
    const fresh = vi.fn();
    const first = callbacks.register("LogSink", { log: old }, ["log"]);
    const second = callbacks.register("LogSink", { log: fresh }, ["log"]);
    await callbacks.receive({
      t: "callback",
      id: 1,
      cb: first.cb,
      method: "log",
      args: [],
    });
    expect(old).not.toHaveBeenCalled();
    expect(main.sent.some((m) => m.t === "logHandoff")).toBe(false);
    await callbacks.receive({
      t: "callback",
      id: 2,
      cb: second.cb,
      method: "log",
      args: [],
    });
    expect(fresh).toHaveBeenCalledTimes(1);
    expect(main.sent.filter((m) => m.t === "logHandoff")).toEqual([
      { t: "logHandoff", id: 2 },
    ]);
    callbacks.clearLogSink();
    await callbacks.receive({
      t: "callback",
      id: 3,
      cb: second.cb,
      method: "log",
      args: [],
    });
    expect(fresh).toHaveBeenCalledTimes(1);
  });

  // verifies: LOG-003, LOG-007, LOG-012
  it("log receipt releases only the named outstanding call and only once", async () => {
    const [, worker] = pair();
    const callbacks = new WorkerCallbacks(worker);
    const handoff = vi.fn();
    const result = callbacks.invoke(1, "log", [], handoff);
    const call = worker.sent[0];
    if (call.t !== "callback") throw new Error("no callback packet");
    callbacks.receiveHandoff(call.id + 1);
    expect(handoff).not.toHaveBeenCalled();
    callbacks.receiveHandoff(call.id);
    callbacks.receiveHandoff(call.id);
    expect(handoff).toHaveBeenCalledTimes(1);
    callbacks.receive({ t: "callbackResult", id: call.id, value: undefined });
    await result;
    callbacks.receiveHandoff(call.id);
    expect(handoff).toHaveBeenCalledTimes(1);
  });

  // verifies: LOG-004, LOG-007, LOG-008
  it("active log stays alive across clear without holding the main callback table", async () => {
    const [main] = pair();
    const finished = vi.fn();
    const callbacks = new MainCallbacks(main, finished);
    let release!: () => void;
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    const sink = callbacks.register(
      "LogSink",
      {
        log: async () => {
          expect(main.sent.at(-1)).toEqual({ t: "logHandoff", id: 1 });
          callbacks.clearLogSink();
          await held;
        },
      },
      ["log"],
    );
    const delivery = callbacks.receive({
      t: "callback",
      id: 1,
      cb: sink.cb,
      method: "log",
      args: [],
    });
    expect(callbacks.hasActiveLog).toBe(true);
    expect(finished).not.toHaveBeenCalled();
    release();
    await delivery;
    expect(callbacks.hasActiveLog).toBe(false);
    expect(finished).toHaveBeenCalledTimes(1);
    expect(main.sent.at(-1)?.t).toBe("callbackResult");
  });
  // verifies: LOG-009
  it("log rejection cannot break error conversion", async () => {
    const [main] = pair();
    const callbacks = new MainCallbacks(main);
    const sink = callbacks.register(
      "LogSink",
      {
        log() {
          throw {
            toString() {
              throw new Error("hostile app error");
            },
          };
        },
      },
      ["log"],
    );
    await callbacks.receive({
      t: "callback",
      id: 1,
      cb: sink.cb,
      method: "log",
      args: [],
    });
    const result = main.sent.at(-1);
    expect(result?.t).toBe("callbackResult");
    if (result?.t !== "callbackResult") throw new Error("no failure result");
    expect(result.error?.message).toBe("log callback failed");
    expect(callbacks.hasActiveLog).toBe(false);
  });
}
