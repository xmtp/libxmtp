import { expect, it, vi } from "vitest";

import { MainCallbacks } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/callbacks.js";
import { WorkerCallbacks } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/worker/callback-stub.js";
import { pair } from "./bridge-support.js";
import { logSinkSetter } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/log-sink.js";
import { MainSession } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/session.js";
import type { CallbackTarget } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/callbacks.js";
import type { CallbackWire } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire.js";
import { waitForLog } from "./logging-wait.js";

async function install(callbacks: MainCallbacks, sink: CallbackTarget) {
  let wire!: CallbackWire;
  await callbacks.updateLogSink(async () => {
    wire = callbacks.register("LogSink", sink, ["log"]);
  });
  return wire;
}

function gate() {
  let release!: () => void;
  const promise = new Promise<void>((resolve) => {
    release = resolve;
  });
  return { promise, release: () => release() };
}

export function registerLoggingTests(): void {
  it("fails a missing log handoff and clears the completed wait timer", async () => {
    vi.useFakeTimers();
    try {
      const missing = waitForLog(
        new Promise<void>(() => {}),
        "missing callback",
      );
      const rejected = expect(missing).rejects.toThrow("missing callback");
      await vi.advanceTimersByTimeAsync(3_000);
      await rejected;
      expect(vi.getTimerCount()).toBe(0);
      expect(await waitForLog(Promise.resolve(7), "unexpected timeout")).toBe(
        7,
      );
      expect(vi.getTimerCount()).toBe(0);
    } finally {
      vi.useRealTimers();
    }
  });

  // verifies: LOG-007, LOG-011
  it("log admission rejects a packet after replace or clear", async () => {
    const [main] = pair();
    const callbacks = new MainCallbacks(main);
    const old = vi.fn();
    const fresh = vi.fn();
    const first = await install(callbacks, { log: old });
    const second = await install(callbacks, { log: fresh });
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

  // verifies: LOG-007, LOG-009, LOG-012
  it("keeps the old sink until ACK and rolls back only a rejected stage", async () => {
    const [main] = pair();
    const callbacks = new MainCallbacks(main);
    const old = vi.fn();
    const next = vi.fn();
    const first = await install(callbacks, { log: old });
    const accepted = gate();
    let second!: CallbackWire;
    const update = callbacks.updateLogSink(async () => {
      second = callbacks.register("LogSink", { log: next }, ["log"]);
      await accepted.promise;
      throw new Error("setter rejected");
    });
    const rejected = expect(update).rejects.toThrow("setter rejected");
    await callbacks.receive({
      t: "callback",
      id: 1,
      cb: first.cb,
      method: "log",
      args: [],
    });
    expect(old).toHaveBeenCalledTimes(1);
    accepted.release();
    await rejected;
    await callbacks.receive({
      t: "callback",
      id: 2,
      cb: first.cb,
      method: "log",
      args: [],
    });
    await callbacks.receive({
      t: "callback",
      id: 3,
      cb: second.cb,
      method: "log",
      args: [],
    });
    expect(old).toHaveBeenCalledTimes(2);
    expect(next).not.toHaveBeenCalled();
    const third = await install(callbacks, { log: next });
    await callbacks.receive({
      t: "callback",
      id: 4,
      cb: first.cb,
      method: "log",
      args: [],
    });
    await callbacks.receive({
      t: "callback",
      id: 5,
      cb: third.cb,
      method: "log",
      args: [],
    });
    expect(old).toHaveBeenCalledTimes(2);
    expect(next).toHaveBeenCalledTimes(1);
  });

  it("counts a staged sink handoff before its setter ACK", async () => {
    const [main] = pair();
    const callbacks = new MainCallbacks(main);
    const accepted = gate();
    const released = gate();
    let sink!: CallbackWire;
    const update = callbacks.updateLogSink(async () => {
      sink = callbacks.register("LogSink", { log: () => released.promise }, [
        "log",
      ]);
      await accepted.promise;
    });
    const delivery = callbacks.receive({
      t: "callback",
      id: 1,
      cb: sink.cb,
      method: "log",
      args: [],
    });
    expect(main.sent.at(-1)).toEqual({ t: "logHandoff", id: 1 });
    expect(callbacks.hasActiveLog).toBe(true);
    accepted.release();
    await update;
    released.release();
    await delivery;
    expect(callbacks.hasActiveLog).toBe(false);
  });

  it("orders concurrent public updates before session selection and recovers after failure", async () => {
    const [main] = pair();
    const session = new MainSession(main, 3, "setters");
    main.emitRaw({ t: "ready", epoch: 1 });
    await session.ready();
    const held = gate();
    const entered = gate();
    const seen: number[] = [];
    const select = vi.fn(
      async (call: (session: MainSession) => Promise<void>) => call(session),
    );
    const update = logSinkSetter<number>(select, async (_session, sink) => {
      seen.push(sink!);
      if (sink === 1) {
        entered.release();
        await held.promise;
        throw new Error("first rejected");
      }
    });
    const first = update(1);
    const rejected = expect(first).rejects.toThrow("first rejected");
    const second = update(2);
    await entered.promise;
    expect(select).toHaveBeenCalledTimes(1);
    expect(seen).toEqual([1]);
    held.release();
    await rejected;
    await second;
    expect(select).toHaveBeenCalledTimes(2);
    expect(seen).toEqual([1, 2]);
    session.terminate();
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
    const log = vi.fn(async () => {
      expect(main.sent.at(-1)).toEqual({ t: "logHandoff", id: 1 });
      callbacks.clearLogSink();
      await held;
    });
    const sink = await install(callbacks, { log });
    const delivery = callbacks.receive({
      t: "callback",
      id: 1,
      cb: sink.cb,
      method: "log",
      args: [],
    });
    await callbacks.receive({
      t: "callback",
      id: 2,
      cb: sink.cb,
      method: "log",
      args: [],
    });
    expect(log).toHaveBeenCalledTimes(1);
    expect(main.sent.filter((message) => message.t === "logHandoff")).toEqual([
      { t: "logHandoff", id: 1 },
    ]);
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
