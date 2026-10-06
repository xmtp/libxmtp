import { describe, expect, it, vi } from "vitest";

import { registerAdminTests } from "./bridge-admin";
import { registerCallbacksTests } from "./bridge-callbacks";
import { registerCreateTests } from "./bridge-create";
import { registerEndingTests } from "./bridge-ending";
import { registerIdentityTests } from "./bridge-identity";
import { registerLogRestartTests } from "./bridge-log-restart";
import { registerLoggingTests } from "./bridge-logging";
import { registerOwnershipTests } from "./bridge-ownership";
import { registerPackageLifetimeTests } from "./bridge-package-lifetime";
import { registerStreamTests } from "./bridge-streams";
import { registerTransportTests } from "./bridge-transport";
import { registerWorkerSessionTests } from "./bridge-worker-sessions";

describe("browser bridge transport", () => {
  registerTransportTests();
  registerOwnershipTests();
  registerCreateTests();
  registerEndingTests();
  registerIdentityTests();
  registerCallbacksTests();
  registerLoggingTests();
  registerLogRestartTests();
  registerAdminTests();
  registerWorkerSessionTests();
  registerPackageLifetimeTests();
  registerStreamTests();
});

// verifies: PROC-046
describe("message stream factories", () => {
  for (const kind of ["all", "group", "dm"] as const) {
    it(`cancels a blocked ${kind} opener through its transport signal`, async () => {
      const { MessageStream } =
        await import("../../../../target/sdk-generated/typescript-wasm/runtime/streams/reader");
      let opened!: () => void;
      const arrived = new Promise<void>((resolve) => {
        opened = resolve;
      });
      let openingSignal: AbortSignal | undefined;
      let aborted = false;
      const messageReader = vi.fn(
        (_selection?: unknown, transport?: { signal: AbortSignal }) => {
          openingSignal = transport?.signal;
          opened();
          return new Promise<{
            next(): Promise<string | undefined>;
            end(): Promise<void>;
          }>((_resolve, reject) => {
            openingSignal?.addEventListener(
              "abort",
              () => {
                aborted = true;
                reject(new Error("opening cancelled"));
              },
              { once: true },
            );
          });
        },
      );
      const source = { messageReader };
      const owner = { conversations: () => source };
      const controller = new AbortController();
      const onClose = vi.fn();
      const options = { signal: controller.signal, onClose };
      const stream =
        kind === "all"
          ? MessageStream.open(owner, undefined, options)
          : kind === "group"
            ? MessageStream.openGroup(owner, source, undefined, options)
            : MessageStream.openDm(owner, source, undefined, options);
      await arrived;
      expect(openingSignal).toBeInstanceOf(AbortSignal);
      expect(openingSignal?.aborted).toBe(false);
      controller.abort();
      expect(openingSignal?.aborted).toBe(true);
      expect(aborted).toBe(true);
      await stream.end();
      expect(await stream.next()).toEqual({ done: true, value: undefined });
      expect(onClose).toHaveBeenCalledExactlyOnceWith({ kind: "closed" });
    });
    it(`forwards ${kind} selection and transport separately`, async () => {
      const { MessageStream } =
        await import("../../../../target/sdk-generated/typescript-wasm/runtime/streams/reader");
      const selection = {
        from: "dc1_exact",
        consentStates: [],
        conversationKind: undefined,
      };
      const end = vi.fn(async () => {});
      const next = vi.fn(async () => "value");
      const messageReader = vi.fn(async () => ({ next, end }));
      const source = { messageReader };
      const owner = { conversations: () => source };
      const controller = new AbortController();
      const onClose = vi.fn();
      const options = { signal: controller.signal, onClose };
      const stream =
        kind === "all"
          ? MessageStream.open(owner, selection, options)
          : kind === "group"
            ? MessageStream.openGroup(owner, source, selection, options)
            : MessageStream.openDm(owner, source, selection, options);
      expect(await stream.next()).toEqual({ done: false, value: "value" });
      expect(messageReader).toHaveBeenCalledWith(selection, {
        signal: expect.any(AbortSignal),
      });
      controller.abort();
      await stream.end();
      expect(end).toHaveBeenCalledTimes(1);
      expect(onClose).toHaveBeenCalledExactlyOnceWith({ kind: "closed" });
      expect(next).toHaveBeenCalledTimes(1);
      expect(await stream.next()).toEqual({ done: true, value: undefined });
    });
  }
});
