import { expect, it, vi } from "vitest";

import {
  LogCallbackQueue,
  MainCallbacks,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/callbacks.js";
import { logSinkSetter } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/log-sink.js";
import type { MainSession } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/session.js";
import { WorkerSessions } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/worker-sessions.js";
import type {
  CallbackWire,
  HandleWire,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire.js";
import { WorkerHost } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/worker/host.js";
import { pair } from "./bridge-support.js";
import { waitForLog } from "./logging-wait.js";

interface Sink {
  log(): Promise<void>;
}
const turn = () => new Promise<void>((resolve) => setTimeout(resolve, 0));
function gate() {
  let release!: () => void;
  return {
    promise: new Promise<void>((resolve) => {
      release = resolve;
    }),
    release: () => release(),
  };
}

export function registerLogRestartTests(): void {
  // verifies: LOG-002, LOG-004, LOG-008, LOG-011
  it("logging-restart: shares callback credit across sessions and cancels closed waiters", async () => {
    const queue = new LogCallbackQueue();
    const endpoints = [pair()[0], pair()[0], pair()[0]];
    const callbacks = endpoints.map(
      (endpoint) => new MainCallbacks(endpoint, undefined, queue),
    );
    const held = gate();
    const calls: number[] = [];
    const wires = callbacks.map((table, index) =>
      table.register(
        "LogSink",
        {
          log: async () => {
            calls.push(index);
            if (index === 0) await held.promise;
            if (index === 2) throw new Error("app log failed");
          },
        },
        ["log"],
      ),
    );
    const receive = (index: number) =>
      callbacks[index].receive({
        t: "callback",
        id: index + 1,
        cb: wires[index].cb,
        method: "log",
        args: [],
      });
    const first = receive(0);
    const abandoned = receive(1);
    const last = receive(2);
    try {
      expect(calls).toEqual([0]);
      expect(endpoints[1].sent).toEqual([]);
      expect(endpoints[2].sent).toEqual([]);
      callbacks[0].close();
      callbacks[1].close();
      await waitForLog(abandoned, "closed callback did not leave the queue");
      expect(calls).toEqual([0]);
      held.release();
      await Promise.all([first, last]);
      expect(calls).toEqual([0, 2]);
      expect(endpoints[2].sent[0]).toEqual({ t: "logHandoff", id: 3 });
      await receive(2);
      expect(calls).toEqual([0, 2, 2]);
    } finally {
      held.release();
      callbacks.forEach((table) => table.close());
      await Promise.all([first, abandoned, last]);
    }
  });

  // verifies: LOG-011
  it("logging-restart: rejects a cleared sink while it waits for another session", async () => {
    const queue = new LogCallbackQueue();
    const endpoints = [pair()[0], pair()[0]];
    const callbacks = endpoints.map(
      (endpoint) => new MainCallbacks(endpoint, undefined, queue),
    );
    const held = gate();
    const stale = vi.fn(() => Promise.resolve());
    let first!: CallbackWire;
    let second!: CallbackWire;
    await callbacks[0].updateLogSink(() => {
      first = callbacks[0].register("LogSink", { log: () => held.promise }, [
        "log",
      ]);
      return Promise.resolve();
    });
    await callbacks[1].updateLogSink(() => {
      second = callbacks[1].register("LogSink", { log: stale }, ["log"]);
      return Promise.resolve();
    });
    const active = callbacks[0].receive({
      t: "callback",
      id: 1,
      cb: first.cb,
      method: "log",
      args: [],
    });
    const waiting = callbacks[1].receive({
      t: "callback",
      id: 2,
      cb: second.cb,
      method: "log",
      args: [],
    });
    try {
      expect(endpoints[1].sent).toEqual([]);
      callbacks[1].clearLogSink();
      held.release();
      await Promise.all([active, waiting]);
      expect(stale).not.toHaveBeenCalled();
      expect(
        endpoints[1].sent.some((message) => message.t === "logHandoff"),
      ).toBe(false);
    } finally {
      held.release();
      callbacks.forEach((table) => table.close());
      await Promise.all([active, waiting]);
    }
  });

  it("logging-restart: cancels a waiter after credit moves but before the app call", async () => {
    const queue = new LogCallbackQueue();
    const firstSignal = new AbortController();
    const secondSignal = new AbortController();
    const held = gate();
    const call = vi.fn(() => Promise.resolve());
    const first = queue.run(firstSignal.signal, () => held.promise);
    const second = queue.run(secondSignal.signal, call);
    const rejected = expect(second).rejects.toThrow("callback was released");
    held.release();
    queueMicrotask(() =>
      secondSignal.abort(new Error("callback was released")),
    );
    await Promise.all([first, rejected]);
    expect(call).not.toHaveBeenCalled();
    await queue.run(firstSignal.signal, call);
    expect(call).toHaveBeenCalledTimes(1);
  });

  for (const change of ["accepted", "rejected", "cleared", "held"] as const) {
    // verifies: LOG-002, LOG-007, LOG-009
    it(`logging-restart: restores the committed package sink after worker retirement (${change})`, async () => {
      const terminated = vi.fn();
      const received = vi.fn(async () => {});
      const holdAck = gate();
      const restoring = gate();
      const rejectedSink = { log: async () => {} };
      const workers: Array<{
        host: WorkerHost;
        sink?: CallbackWire;
        configurations: string[];
      }> = [];
      const sessions = new WorkerSessions(
        () => {
          const [main, worker] = pair();
          main.terminate = terminated;
          const state: {
            host: WorkerHost;
            sink?: CallbackWire;
            configurations: string[];
          } = {
            configurations: [],
            host: new WorkerHost(
              worker,
              3,
              "restart",
              async () => {},
              async (key, args, context) => {
                if (key === "configure") {
                  if (args[0] === "bad")
                    throw new Error("configuration rejected");
                  state.configurations.push(args[0] as string);
                  return;
                }
                if (key === "setSink") {
                  if (state.configurations.length === 0)
                    throw new Error("logging is not initialized");
                  if (change === "held" && workers.length === 2) {
                    restoring.release();
                    await holdAck.promise;
                  }
                  state.sink = args[0] as CallbackWire | undefined;
                  return;
                }
                if (key === "emit") {
                  if (state.sink)
                    await state.host.callbacks.invoke(state.sink.cb, "log", []);
                  return;
                }
                return context.registry.add(
                  { end: async () => {} },
                  "StorageAdmin",
                );
              },
            ),
          };
          workers.push(state);
          return main;
        },
        3,
        "restart",
        async (session: MainSession) => {
          await update.initialize(session);
        },
      );
      const update = logSinkSetter<Sink>(
        async (call) => call(await sessions.get()),
        async (session, sink) => {
          await session.call("setSink", () => {
            const wire = sink
              ? session.callbacks.register(
                  "LogSink",
                  { log: () => sink.log() },
                  ["log"],
                )
              : undefined;
            if (sink === rejectedSink) throw new Error("setter rejected");
            return [wire];
          });
        },
      );
      try {
        await update.configure(async (session) => {
          await session.call("configure", ["first"]);
        });
        await update.configure(async (session) => {
          await session.call("configure", ["last"]);
        });
        await expect(
          update.configure(async (session) => {
            await session.call("configure", ["bad"]);
          }),
        ).rejects.toThrow("configuration rejected");
        await update({ log: received });
        if (change === "rejected")
          await expect(update(rejectedSink)).rejects.toThrow("setter rejected");
        if (change === "cleared") await update();
        const first = await sessions.create(async (session) => ({
          session,
          handle: (await session.call("open", [])) as HandleWire,
        }));
        await first.session.call("emit", []);
        expect(received).toHaveBeenCalledTimes(change === "cleared" ? 0 : 1);
        first.session.release([first.handle.h]);
        await turn();
        expect(terminated).toHaveBeenCalledTimes(1);
        const factory = vi.fn(async (session: MainSession) => ({
          session,
          handle: (await session.call("open", [])) as HandleWire,
        }));
        const opening = sessions.create(factory);
        if (change === "held") {
          await waitForLog(
            restoring.promise,
            "sink restoration did not reach its ACK",
          );
          expect(factory).not.toHaveBeenCalled();
          holdAck.release();
        }
        const second = await opening;
        expect(workers).toHaveLength(2);
        expect(workers[1].configurations).toEqual(["first", "last"]);
        await second.session.call("emit", []);
        expect(received).toHaveBeenCalledTimes(change === "cleared" ? 0 : 2);
        second.session.release([second.handle.h]);
        await turn();
        expect(terminated).toHaveBeenCalledTimes(2);
      } finally {
        holdAck.release();
        sessions.terminate();
      }
    });
  }

  it("logging-restart: holds factories behind generation initialization and terminates a failed initializer", async () => {
    const hold = gate();
    const entered = gate();
    const terminated = vi.fn();
    let generations = 0;
    let reject = true;
    const factory = vi.fn(() => Promise.resolve(42));
    const sessions = new WorkerSessions(
      () => {
        generations++;
        const [main, worker] = pair();
        main.terminate = terminated;
        new WorkerHost(
          worker,
          3,
          "init",
          () => Promise.resolve(),
          () => Promise.resolve(undefined),
        );
        return main;
      },
      3,
      "init",
      async () => {
        entered.release();
        await hold.promise;
        if (reject) throw new Error("restore rejected");
      },
    );
    try {
      const opening = sessions.create(factory);
      const failure = opening.then(
        (value) => ({ value }),
        (error: unknown) => ({ error }),
      );
      await waitForLog(entered.promise, "initializer did not start");
      expect(factory).not.toHaveBeenCalled();
      hold.release();
      expect(await failure).toEqual({ error: new Error("restore rejected") });
      expect(terminated).toHaveBeenCalledTimes(1);
      reject = false;
      expect(await sessions.create(factory)).toBe(42);
      expect(generations).toBe(2);
      expect(factory).toHaveBeenCalledTimes(1);
    } finally {
      hold.release();
      sessions.terminate();
    }
  });
}
