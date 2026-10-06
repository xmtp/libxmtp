import { describe, expect, it } from "vitest";

import { WorkerSessions } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/worker-sessions";
import {
  bridgeError,
  encodeError,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire";
import { Endpoint } from "./bridge-support";

export function registerWorkerSessionTests(): void {
  describe("worker generations", () => {
    function setup() {
      const endpoints: Endpoint[] = [];
      let terminated = 0;
      const sessions = new WorkerSessions(
        () => {
          const endpoint = new Endpoint();
          endpoint.terminate = () => {
            terminated++;
          };
          endpoints.push(endpoint);
          return endpoint;
        },
        2,
        "generations",
      );
      return { endpoints, sessions, terminated: () => terminated };
    }

    it("shares concurrent first openings", async () => {
      const { endpoints, sessions } = setup();
      const first = sessions.get();
      const second = sessions.get();
      expect(endpoints).toHaveLength(1);
      endpoints[0].emitRaw({ t: "ready", epoch: 1 });
      expect(await first).toBe(await second);
      sessions.terminate();
    });

    it("keeps a replacement when an older opening fails", async () => {
      const { endpoints, sessions, terminated } = setup();
      const first = sessions.get();
      const failure = expect(first).rejects.toMatchObject({
        code: "WorkerTerminated",
      });
      endpoints[0].emitRaw({
        t: "fatal",
        error: encodeError(new Error("initialization failed")),
      });
      const second = sessions.get();
      expect(endpoints).toHaveLength(2);
      endpoints[1].emitRaw({ t: "ready", epoch: 2 });
      await failure;
      const live = await second;
      endpoints[0].emitRaw({ t: "ready", epoch: 99 });
      endpoints[0].emitRaw({
        t: "fatal",
        error: encodeError(new Error("late failure")),
      });
      const reused = sessions.get();
      expect(endpoints).toHaveLength(2);
      expect(await reused).toBe(live);
      expect(live.isTerminated).toBe(false);
      expect(terminated()).toBe(1);
      sessions.terminate();
    });

    it("terminates before returning the original failed-call error", async () => {
      const { endpoints, sessions, terminated } = setup();
      const opening = sessions.get();
      endpoints[0].emitRaw({ t: "ready", epoch: 1 });
      const session = await opening;
      const call = session.call("create", []);
      await Promise.resolve();
      const sent = endpoints[0].sent.findLast(
        (message) => message.t === "call",
      );
      if (sent?.t !== "call") throw new Error("call was not sent");
      endpoints[0].emitRaw({
        t: "error",
        id: sent.id,
        error: encodeError(bridgeError("storageBusy")),
        fatal: true,
      });
      await expect(call).rejects.toMatchObject({
        code: "StorageBusy",
        retryable: true,
      });
      expect(session.isTerminated).toBe(true);
      expect(terminated()).toBe(1);
      const replacement = sessions.get();
      expect(endpoints).toHaveLength(2);
      endpoints[1].emitRaw({ t: "ready", epoch: 2 });
      expect(await replacement).not.toBe(session);
      sessions.terminate();
    });
  });
}
