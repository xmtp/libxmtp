import { expect, it, vi } from "vitest";

import { logSinkSetter } from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/main/log-sink.js";
import type { MainSession } from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/main/session.js";
import { WorkerSessions } from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/main/worker-sessions.js";
import type {
  CallbackWire,
  HandleWire,
} from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/wire.js";
import { WorkerHost } from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/worker/host.js";
import { waitForLog } from "../ts/logging-wait.js";
import { pair } from "./bridge-support.js";

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
