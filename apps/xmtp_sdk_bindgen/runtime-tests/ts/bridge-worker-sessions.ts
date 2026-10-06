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

    it("rejects migration while a client creation is reserved", async () => {
      const { endpoints, sessions } = setup();
      const creation = sessions.create(async () => 3);
      await expect(sessions.runExclusive(async () => 4)).rejects.toMatchObject({
        code: "StorageBusy",
      });
      expect(endpoints).toHaveLength(1);
      endpoints[0].emitRaw({ t: "ready", epoch: 1 });
      expect(await creation).toBe(3);
      sessions.terminate();
    });

    it("keeps migration exclusive until actual termination completes", async () => {
      const { endpoints, sessions } = setup();
      let release!: () => void;
      let started!: () => void;
      const stopping = new Promise<void>((resolve) => {
        release = resolve;
      });
      const stopped = new Promise<void>((resolve) => {
        started = resolve;
      });
      let completed = false;
      const migration = sessions
        .runExclusive(async () => 42)
        .then((value) => {
          completed = true;
          return value;
        });
      endpoints[0].terminate = () => {
        started();
        return stopping;
      };
      endpoints[0].emitRaw({ t: "ready", epoch: 1 });
      await stopped;
      expect(completed).toBe(false);
      await expect(sessions.get()).rejects.toMatchObject({
        code: "StorageBusy",
      });
      await expect(sessions.create(async () => 7)).rejects.toMatchObject({
        code: "StorageBusy",
      });
      await expect(sessions.runExclusive(async () => 8)).rejects.toMatchObject({
        code: "StorageBusy",
      });
      expect(endpoints).toHaveLength(1);
      release();
      expect(await migration).toBe(42);
      const retry = sessions.runExclusive(async () => 9);
      endpoints[1].emitRaw({ t: "ready", epoch: 2 });
      expect(await retry).toBe(9);
    });

    it("releases migration admission after a failed call", async () => {
      const { endpoints, sessions, terminated } = setup();
      const failure = sessions.runExclusive(async () => {
        throw new Error("migration failed");
      });
      const rejected = expect(failure).rejects.toThrow("migration failed");
      endpoints[0].emitRaw({ t: "ready", epoch: 1 });
      await rejected;
      expect(terminated()).toBe(1);
      const retry = sessions.create(async () => 9);
      endpoints[1].emitRaw({ t: "ready", epoch: 2 });
      expect(await retry).toBe(9);
      sessions.terminate();
    });

    it("accepts migration after a completed scalar call before the idle message", async () => {
      const { endpoints, sessions, terminated } = setup();
      const scalar = sessions.create((session) =>
        session.call("readMigrationArchive", []),
      );
      endpoints[0].emitRaw({ t: "ready", epoch: 1 });
      await new Promise((resolve) => setTimeout(resolve, 0));
      const sent = endpoints[0].sent.findLast(
        (message) => message.t === "call",
      );
      if (sent?.t !== "call") throw new Error("call was not sent");
      endpoints[0].emitRaw({
        t: "return",
        id: sent.id,
        value: new Uint8Array([1]),
      });
      expect(await scalar).toEqual(new Uint8Array([1]));
      const migration = sessions.runExclusive(async () => 42);
      void migration.catch(() => {});
      expect(endpoints).toHaveLength(2);
      endpoints[1].emitRaw({ t: "ready", epoch: 2 });
      expect(await migration).toBe(42);
      expect(terminated()).toBe(2);
    });

    it("replaces a failed generation after actual termination and ignores late events", async () => {
      const { endpoints, sessions } = setup();
      const opening = sessions.get();
      endpoints[0].emitRaw({ t: "ready", epoch: 1 });
      const failed = await opening;
      let release!: () => void;
      const stopped = new Promise<void>((resolve) => {
        release = resolve;
      });
      endpoints[0].terminate = () => stopped;
      endpoints[0].emitRaw({
        t: "fatal",
        error: encodeError(new Error("worker failed")),
      });
      expect(failed.isTerminated).toBe(true);
      let called = false;
      const migration = sessions.runExclusive(async (session) => {
        called = true;
        expect(session).not.toBe(failed);
        endpoints[0].emitRaw({ t: "ready", epoch: 99 });
        endpoints[0].emitRaw({
          t: "fatal",
          error: encodeError(new Error("late failure")),
        });
        expect(session.isTerminated).toBe(false);
        return { groupCount: 2n, messageCount: 3n, consentCount: 1n };
      });
      void migration.catch(() => {});
      await new Promise((resolve) => setTimeout(resolve, 0));
      expect(endpoints).toHaveLength(1);
      expect(called).toBe(false);
      release();
      await new Promise((resolve) => setTimeout(resolve, 0));
      expect(endpoints).toHaveLength(2);
      endpoints[1].emitRaw({ t: "ready", epoch: 2 });
      expect(await migration).toEqual({
        groupCount: 2n,
        messageCount: 3n,
        consentCount: 1n,
      });
    });

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
