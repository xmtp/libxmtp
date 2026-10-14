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
describe("reader stream opening", () => {
  it("cancels a blocked opener through its transport signal", async () => {
    const { ReaderStream } =
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
    const owner = {};
    const controller = new AbortController();
    const onClose = vi.fn();
    const options = { signal: controller.signal, onClose };
    const stream = new ReaderStream(
      (signal) => source.messageReader(undefined, { signal }),
      owner,
      options,
    );
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
  it("closes the reader after its opener receives the transport signal", async () => {
    const { ReaderStream } =
      await import("../../../../target/sdk-generated/typescript-wasm/runtime/streams/reader");
    const selection = {
      from: "dc1_exact",
      consentStates: [],
      conversationKind: undefined,
    };
    const end = vi.fn(async () => {});
    const next = vi.fn(async () => "value");
    const messageReader = vi.fn(
      async (_selection: unknown, _transport: { signal: AbortSignal }) => ({
        next,
        end,
      }),
    );
    const source = { messageReader };
    const owner = {};
    const controller = new AbortController();
    const onClose = vi.fn();
    const options = { signal: controller.signal, onClose };
    const stream = new ReaderStream(
      (signal) => source.messageReader(selection, { signal }),
      owner,
      options,
    );
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
});
