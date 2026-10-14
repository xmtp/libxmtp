import { expect, it } from "vitest";

import {
  Message,
  registerClient,
} from "../../../../target/sdk-generated/typescript-wasm/host-message.gen.js";
import { UniffiInternalError } from "../../../../target/sdk-generated/typescript-wasm/node_modules/@ubjs/core/dist/esm/index.js";
import type { Client } from "../../../../target/sdk-generated/typescript-wasm/proxy.gen.js";
import {
  ValueCodec,
  type Layouts,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/codec.js";
import { MainSession } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/session.js";
import {
  BRIDGE_ERROR_CODES,
  BridgeError,
  assertCloneable,
  bridgeError,
  decodeError,
  encodeError,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire.js";
import { encodeError as encodeGeneratedError } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire.js";
import {
  Compression,
  CredentialError_Tags,
  EncodedContent,
  ErrorCategory,
  MessageContent,
  ListenerError_Tags,
  LogSinkError_Tags,
  PreAuthenticateError_Tags,
  SignerError_Tags,
  XmtpError,
  XmtpError_Tags,
  type MessageData,
  type SendOptions,
} from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk.js";
import { setLogSink } from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk.js";
import { Endpoint, host, TestProxy, stringKeys } from "./bridge-support";
export function registerTransportTests(): void {
  it("exposes only PascalCase error codes", () => {
    const exposed = [
      CredentialError_Tags,
      ListenerError_Tags,
      LogSinkError_Tags,
      PreAuthenticateError_Tags,
      SignerError_Tags,
      XmtpError_Tags,
    ].flatMap((variants) => Object.values(variants));
    for (const code of BRIDGE_ERROR_CODES) {
      const error = bridgeError(code);
      exposed.push(error.code, encodeError(error).code);
    }
    exposed.push(encodeError(new Error("unknown failure")).code);
    for (const code of exposed) {
      expect(code).toMatch(/^[A-Z][A-Za-z0-9]*$/);
    }
  });

  it("uses the generated Unknown category for fallback errors", () => {
    for (const error of [
      Object.assign(new Error("missing detail"), { tag: "XmtpError" }),
      new Error("plain failure"),
      "non-error failure",
    ]) {
      expect(encodeError(error).category).toBe(ErrorCategory.Unknown);
    }
  });

  it("preserves structured error fields independently of diagnostic text", () => {
    for (const message of [
      "invalid lowercase hex ID",
      "different diagnostic",
    ]) {
      const details = {
        code: "InvalidArgument",
        category: ErrorCategory.Input,
        retryable: false,
        message,
      };
      expect(encodeError(new XmtpError.InvalidArgument(details))).toMatchObject(
        {
          variant: "InvalidArgument",
          code: "InvalidArgument",
          category: ErrorCategory.Input,
          retryable: false,
          details: [details],
        },
      );
    }
  });

  it("does not classify rendered ID errors as structured failures", () => {
    const cause =
      'invalid argument: ErrorDetails { code: "InvalidArgument", category: Input, retryable: false, message: "invalid lowercase hex ID", stream_failure: None }';
    const malformed = new Error(
      `Failed to convert arg 'id':\nLifting custom type \`xmtp_sdk::ids::MessageId\` from FFI type \`alloc::string::String\` failed\n\nCaused by:\n    ${cause}`,
    );
    expect(encodeError(malformed).code).toBe("Unknown");
    expect(
      encodeError(new Error(`Lifting custom type \`OtherId\` failed: ${cause}`))
        .code,
    ).toBe("Unknown");
  });

  it("encodes an aborted binding call as Cancelled", () => {
    const aborted = new UniffiInternalError.AbortError();
    // Cancellation comes from the class, even when diagnostic text changes.
    aborted.name = "renamed binding failure";
    expect(encodeError(aborted)).toMatchObject({
      variant: "Cancelled",
      code: "Cancelled",
      category: 6,
      retryable: false,
    });
  });

  it("encodes a real cancelled generated binding call as Cancelled", async () => {
    const { uniffiInitAsync } =
      await import("../../../../target/sdk-generated/typescript-wasm/binding.js");
    await uniffiInitAsync(
      new URL(
        "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk.wasm",
        import.meta.url,
      ),
    );
    const controller = new AbortController();
    controller.abort();
    let failure: unknown;
    try {
      await setLogSink(undefined, { signal: controller.signal });
    } catch (error) {
      failure = error;
    }
    expect(failure).toBeInstanceOf(UniffiInternalError.AbortError);
    const expected = {
      variant: "Cancelled",
      code: "Cancelled",
      category: 6,
      retryable: false,
      message: "A Rust future was aborted",
    };
    expect(encodeGeneratedError(failure)).toMatchObject(expected);
    expect(encodeError(failure)).toMatchObject(expected);
  });

  it("uses the numeric Lifecycle category for transport errors", () => {
    for (const code of [
      "contractMismatch",
      "workerTerminated",
      "cancelled",
    ] as const)
      expect(bridgeError(code).category).toBe(6);
  });

  it("uses the Rust ClientClosed fields for bridge lifecycle errors", () => {
    expect(bridgeError("clientClosed")).toMatchObject({
      variant: "ClientClosed",
      code: "ClientClosed",
      category: 6,
      retryable: false,
      details: [
        {
          code: "ClientClosed",
          category: 6,
          retryable: false,
          message: "client is closed",
        },
      ],
    });
  });

  it("uses the Rust StorageBusy fields for bridge lock errors", () => {
    expect(bridgeError("storageBusy")).toMatchObject({
      variant: "StorageBusy",
      code: "StorageBusy",
      category: 2,
      retryable: true,
    });
  });

  it("uses the XmtpError.Lagged variant for bridge stream errors", () => {
    expect(bridgeError("lagged")).toMatchObject({
      variant: "Lagged",
      code: "Lagged",
      category: 9,
      retryable: true,
    });
  });

  it("XmtpError keeps variant and detail fields", () => {
    const error = new BridgeError(
      "StorageBusy",
      "StorageBusy",
      "storage",
      true,
      "busy",
      { pool: "one" },
    );
    const result = decodeError(structuredClone(encodeError(error)));
    expect(result).toMatchObject({
      variant: "StorageBusy",
      code: "StorageBusy",
      category: "storage",
      retryable: true,
      details: { pool: "one" },
    });
    class TaggedError extends Error {
      readonly tag = "StorageBusy";
      readonly inner = [
        { code: "StorageBusy", category: 2, retryable: true, pool: "one" },
      ];
    }
    const tagged = decodeError(
      structuredClone(encodeError(new TaggedError("busy"))),
    );
    expect(tagged).toMatchObject({
      variant: "StorageBusy",
      code: "StorageBusy",
      category: 2,
      retryable: true,
      details: [{ pool: "one" }],
    });
  });

  it("object handles return the same worker object", () => {
    const { engine } = host(async () => undefined);
    const layouts: Layouts = { records: {}, enums: {} };
    const object = { marker: 1 };
    const encoder = new ValueCodec(
      layouts,
      "worker",
      "encode",
      undefined,
      engine.registry,
    );
    const decoder = new ValueCodec(
      layouts,
      "worker",
      "decode",
      undefined,
      engine.registry,
    );
    const shape = { kind: "object", name: "Group" } as const;
    const encoded = encoder.convert(shape, object);
    expect(decoder.convert(shape, structuredClone(encoded))).toBe(object);
  });

  it("replies with send options that leave optimistic at its Rust default", async () => {
    const session = new MainSession(new Endpoint(), 1, "same");
    const sent: unknown[] = [];
    const client = {
      clientKey: () => 7n,
      conversations: () => ({
        replyToMessage: async (
          _id: unknown,
          _content: unknown,
          options: unknown,
        ) => {
          sent.push(options);
          return "01".repeat(32);
        },
      }),
    } as unknown as Client;
    registerClient(session, client, []);
    const message = new Message(
      {
        id: "00".repeat(32),
        clientKey: 7n,
        content: MessageContent.Text.new("parent"),
      } as MessageData,
      session,
    );
    const content = EncodedContent.create({
      type: {
        authorityId: "xmtp.org",
        typeId: "text",
        versionMajor: 1,
        versionMinor: 0,
      },
      content: new Uint8Array([104, 105]).buffer,
    });
    // A JavaScript caller can leave out any field that has a Rust default.
    const options = { compression: Compression.Gzip } as SendOptions;
    await message.reply(content, options);
    expect(sent).toEqual([
      { optimistic: false, compression: Compression.Gzip },
    ]);
    for (const invalid of [
      null,
      [],
      { optimistic: "yes" },
      { compression: 9 },
      { idempotencyKey: 1 },
    ])
      await expect(
        message.reply(content, invalid as SendOptions),
      ).rejects.toThrow("invalid send options");
    expect(sent).toHaveLength(1);
    expect(client.clientKey()).toBe(7n);
  });

  it("encodes a Bytes view as an ArrayBuffer with only the view bytes", () => {
    const encoder = new ValueCodec(
      { records: {}, enums: {} },
      "main",
      "encode",
    );
    const view = new Uint8Array([9, 1, 2, 3, 9]).subarray(1, 4);
    const encoded = encoder.convert({ kind: "value", type: "Bytes" }, view);
    expect(encoded).toBeInstanceOf(ArrayBuffer);
    expect([...new Uint8Array(encoded as ArrayBuffer)]).toEqual([1, 2, 3]);
  });

  it("rolls back sibling handles when a later snapshot throws", () => {
    const { engine } = host(async () => undefined);
    const registry = engine.registry;
    expect(() =>
      registry.scope(() => [
        registry.add({}, "Group"),
        registry.add({}, "Group", undefined, () => {
          throw new Error("snapshot failed");
        }),
      ]),
    ).toThrow("snapshot failed");
    expect(registry.size).toBe(0);
  });

  it("rolls back nested handles when a snapshot throws", () => {
    const { engine } = host(async () => undefined);
    const registry = engine.registry;
    const kept = registry.add({}, "Group");
    expect(() =>
      registry.add({}, "Client", undefined, (owner) => {
        registry.add({}, "Conversations", owner, (nestedOwner) => {
          registry.add({}, "Group", nestedOwner);
          return {};
        });
        throw new Error("snapshot failed");
      }),
    ).toThrow("snapshot failed");
    expect(registry.size).toBe(1);
    expect(registry.release([kept.h])).toEqual([kept.owner]);
    expect(registry.size).toBe(0);
  });

  it("uses a new epoch for each worker and rejects a proxy from another session", async () => {
    const first = host(async () => undefined);
    const second = host(async () => undefined);
    await Promise.all([first.session.ready(), second.session.ready()]);
    expect(first.engine.registry.epoch).not.toBe(second.engine.registry.epoch);
    const proxy = new TestProxy(
      first.session,
      first.engine.registry.add({}, "Group"),
    );
    const encoder = new ValueCodec(
      { records: {}, enums: {} },
      "main",
      "encode",
      second.session,
    );
    expect(() =>
      encoder.convert({ kind: "object", name: "Group" }, proxy),
    ).toThrow("clientClosed");
  });

  it("batches release messages and transfers returned bytes", async () => {
    const { main, worker, session } = host(async () => ({
      bytes: new Uint8Array([1, 2, 3]),
    }));
    await session.ready();
    session.release([11]);
    session.release([12]);
    await new Promise<void>((resolve) => queueMicrotask(resolve));
    expect(main.sent.filter((message) => message.t === "release")).toEqual([
      { t: "release", handles: [11, 12], revision: 1 },
    ]);
    await session.call("bytes", []);
    expect(
      worker.transfers.some((transfer) =>
        transfer.some((item) => item instanceof ArrayBuffer),
      ),
    ).toBe(true);
  });

  it("accepts sets in cloneable wire values", () => {
    expect(() =>
      assertCloneable(new Set([1n, new Uint8Array([2])])),
    ).not.toThrow();
  });

  it("fails pending calls on an unknown wire message", async () => {
    const { main, session } = host(async () => new Promise<unknown>(() => {}));
    await session.ready();
    const pending = session.call("waiting", []);
    await Promise.resolve();
    main.emitRaw({ t: "futureMessage" });
    await expect(pending).rejects.toMatchObject({ code: "ContractMismatch" });
  });

  it("worker_death_settles_pending", async () => {
    const { main, session } = host(async () => new Promise<unknown>(() => {}));
    await session.ready();
    const call = session.call("wait", []);
    await Promise.resolve();
    expect(
      main.sent.some(
        (message) => message.t === "call" && message.key === "wait",
      ),
    ).toBe(true);
    expect(Reflect.get(session, "pending").size).toBe(1);
    main.exit();
    await expect(
      Promise.race([
        call,
        new Promise<never>((_resolve, reject) =>
          setTimeout(
            () => reject(new Error("pending call did not settle")),
            100,
          ),
        ),
      ]),
    ).rejects.toMatchObject({ code: "WorkerTerminated" });
  });

  it("rolls back a pending call when structuredClone throws", async () => {
    const { session } = host(async () => undefined);
    await session.ready();
    await expect(
      session.call("cannot-clone", [() => undefined]),
    ).rejects.toThrow();
    expect(Reflect.get(session, "pending").size).toBe(0);
  });

  it("panic_closes_clients", async () => {
    const { engine, session } = host(
      async () => new Promise<unknown>(() => {}),
    );
    await session.ready();
    const handle = engine.registry.add({}, "Client");
    engine.registry.add({}, "Group", handle.owner);
    const proxy = new TestProxy(session, handle);
    expect(engine.registry.size).toBe(2);
    const call = proxy.ping();
    engine.fatal(new Error("panic"));
    await expect(call).rejects.toMatchObject({ code: "WorkerTerminated" });
    expect(() => proxy.ping()).toThrowError(BridgeError);
    expect(() => proxy.ping()).toThrow("clientClosed");
  });

  it("release_on_end_and_gc", async () => {
    const { engine, session } = host(async () => undefined);
    await session.ready();
    const handle = engine.registry.add({}, "Client");
    engine.registry.add({}, "Group", handle.owner);
    const proxy = new TestProxy(session, handle);
    expect(engine.registry.size).toBe(2);
    await proxy.end();
    await new Promise<void>((resolve) => queueMicrotask(resolve));
    expect(engine.registry.size).toBe(0);
    expect(() => proxy.ping()).toThrow("clientClosed");
  });

  it("keeps the owner release hook off every proxy", async () => {
    const { engine, session } = host(async () => undefined);
    await session.ready();
    const proxy = new TestProxy(session, engine.registry.add({}, "Client"));
    expect("endOwner" in proxy).toBe(false);
    expect(stringKeys(proxy)).not.toContain("endOwner");
  });
}
