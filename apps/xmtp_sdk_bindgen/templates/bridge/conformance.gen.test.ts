import { execFileSync } from "node:child_process";
import { serialize } from "node:v8";

import { describe, expect, it } from "vitest";

import { mainDecoder, mainEncoder } from "./codec.main.gen.js";
import { workerDecoder, workerEncoder } from "./codec.worker.gen.js";
import * as P from "./proxy.gen.js";
import { registerForeign } from "./reverse.gen.js";
import { enumFactory, type Shape } from "./runtime/bridge/codec.js";
import { RemoteObject } from "./runtime/bridge/main/remote-object.js";
import { MainSession } from "./runtime/bridge/main/session.js";
import type { WireEndpoint, WireMessage } from "./runtime/bridge/wire.js";
import { WorkerHost } from "./runtime/bridge/worker/host.js";
import { foreignStub } from "./stubs.gen.js";
import { BRIDGED_OBJECTS, FOREIGN_OBJECTS, LAYOUTS } from "./wire.gen.js";
import * as B from "./xmtp_sdk.js";

class Endpoint implements WireEndpoint {
  peer?: Endpoint;
  readonly sent: WireMessage[] = [];
  private receive: (message: WireMessage) => void = () => {};
  postMessage(message: WireMessage): void {
    this.sent.push(message);
    const copy = structuredClone(message);
    queueMicrotask(() => this.peer?.receive(copy));
  }
  onMessage(handler: (message: WireMessage) => void): void {
    this.receive = handler;
  }
  onExit(): void {}
}

function endpoints(): [Endpoint, Endpoint] {
  const main = new Endpoint();
  const worker = new Endpoint();
  main.peer = worker;
  worker.peer = main;
  return [main, worker];
}

function sample(shape: Shape, seed: number): unknown {
  switch (shape.kind) {
    case "value":
      switch (shape.type) {
        case "Boolean":
          return seed % 2 === 0;
        case "String":
          return `sample-${seed}`;
        case "Bytes":
          return new Uint8Array([seed, 0, 255]).buffer;
        case "UInt64":
        case "Int64":
          return 9007199254740993n + BigInt(seed);
        case "Timestamp":
          return new Date(1700000000000 + seed);
        case "Duration":
          return 1000 + seed;
        case undefined:
          return undefined;
        default:
          return seed + 17;
      }
    case "object":
    case "foreign":
    case "callback":
      return { marker: shape.name, seed };
    case "record": {
      const layout = LAYOUTS.records[shape.name];
      if (!layout) throw new Error(`unknown record ${shape.name}`);
      const result: Record<string, unknown> = {};
      for (const [index, [name, field]] of Object.entries(
        layout.fields,
      ).entries())
        result[name] = sample(field, seed + index);
      return result;
    }
    case "enum": {
      const layout = LAYOUTS.enums[shape.name];
      if (!layout) throw new Error(`unknown enum ${shape.name}`);
      const variants = Object.entries(layout.variants);
      const entry = variants[seed % variants.length];
      if (!entry) throw new Error(`empty enum ${shape.name}`);
      if (layout.flat) return seed % variants.length;
      if (layout.error) {
        const fields = entry[1];
        if (!Array.isArray(fields)) throw new Error("invalid error layout");
        return enumFactory(B)(
          shape.name,
          entry[0],
          fields[0] ? [sample(fields[0], seed)] : [],
        );
      }
      return enumSample(shape.name, entry[0], entry[1], seed);
    }
    case "optional":
      return seed % 2 === 0 ? sample(shape.inner, seed) : undefined;
    case "sequence":
      return [sample(shape.inner, seed), sample(shape.inner, seed + 2)];
    case "set":
      return new Set([sample(shape.inner, seed)]);
    case "map":
      return new Map([[sample(shape.key, seed), sample(shape.value, seed)]]);
  }
}

function enumSample(
  name: string,
  tag: string,
  fields: Record<string, Shape> | Shape[],
  seed: number,
): unknown {
  if (Array.isArray(fields))
    return enumFactory(B)(
      name,
      tag,
      fields.map((field) => sample(field, seed)),
    );
  const inner: Record<string, unknown> = {};
  for (const [field, shape] of Object.entries(fields))
    inner[field] = sample(shape, seed);
  return enumFactory(B)(name, tag, inner);
}

async function roundTrip(
  shape: Shape,
  original: unknown,
  mutate?: (wire: unknown) => unknown,
  mutateRestored?: (value: unknown) => unknown,
): Promise<void> {
  const [main, worker] = endpoints();
  const host = new WorkerHost(
    worker,
    1,
    "conformance",
    async () => {},
    async () => undefined,
  );
  const session = new MainSession(main, 1, "conformance");
  await session.ready();
  const workerOut = workerEncoder(host.registry);
  const workerIn = workerDecoder(host.registry, host.callbacks, enumFactory(B));
  const mainOut = mainEncoder(session);
  const mainIn = mainDecoder(
    session,
    enumFactory(B),
    (handle) => session.proxy(handle) ?? new RemoteObject(session, handle),
  );
  const wire = workerOut.convert(shape, original);
  if (shape.kind === "enum" && LAYOUTS.enums[shape.name]?.error) {
    expect(wire !== null && typeof wire === "object").toBe(true);
    if (wire !== null && typeof wire === "object")
      expect(Array.isArray(Reflect.get(wire, "details"))).toBe(true);
  }
  const received = mutate
    ? mutate(structuredClone(wire))
    : structuredClone(wire);
  const generated =
    shape.kind === "record" || shape.kind === "enum" || shape.kind === "object"
      ? Reflect.get(
          P,
          `decode${shape.kind[0].toUpperCase()}${shape.kind.slice(1)}${shape.name}`,
        )
      : undefined;
  if (
    (shape.kind === "record" ||
      shape.kind === "enum" ||
      shape.kind === "object") &&
    shape.name !== "BridgeProperty"
  )
    expect(typeof generated).toBe("function");
  const mainValue =
    typeof generated === "function" && !containsForeign(shape, original)
      ? Reflect.apply(generated, undefined, [session, received])
      : mainIn.convert(shape, received);
  const returned = mainOut.convert(shape, mainValue);
  const decoded = workerIn.convert(shape, structuredClone(returned));
  const restored = mutateRestored ? mutateRestored(decoded) : decoded;
  const semantic = (value: unknown): unknown =>
    value instanceof Error && "tag" in value
      ? { tag: value.tag, inner: "inner" in value ? value.inner : undefined }
      : value;
  expect(
    Buffer.compare(
      serialize(semantic(restored)),
      serialize(semantic(original)),
    ),
  ).toBe(0);
  if (shape.kind === "object") expect(restored).toBe(original);
}

function containsForeign(shape: Shape, value: unknown): boolean {
  if (value === undefined || value === null) return false;
  switch (shape.kind) {
    case "foreign":
    case "callback":
      return true;
    case "record": {
      const fields = LAYOUTS.records[shape.name]?.fields;
      if (!fields || typeof value !== "object") return false;
      return Object.entries(fields).some(([name, field]) =>
        containsForeign(field, Reflect.get(value, name)),
      );
    }
    case "optional":
      return containsForeign(shape.inner, value);
    case "sequence":
      return (
        Array.isArray(value) &&
        value.some((item) => containsForeign(shape.inner, item))
      );
    case "set":
      return (
        value instanceof Set &&
        Array.from(value).some((item) => containsForeign(shape.inner, item))
      );
    case "map":
      return (
        value instanceof Map &&
        Array.from(value).some(
          ([key, item]) =>
            containsForeign(shape.key, key) ||
            containsForeign(shape.value, item),
        )
      );
    case "enum":
    case "object":
    case "value":
      return false;
  }
}

describe("generated bridge value conformance", () => {
  it("loads the real WASM bridge in worker_threads", () => {
    const output = execFileSync(
      "sdks/node/node_modules/.bin/tsx",
      ["crates/xmtp_sdk/conformance/browser/bridge.real.mts"],
      {
        cwd: process.cwd(),
        env: {
          ...process.env,
          NODE_OPTIONS: "--preserve-symlinks --expose-gc",
        },
        encoding: "utf8",
        timeout: 120000,
      },
    );
    expect(output).toContain("real WASM client");
  }, 120000);

  for (const type of [
    "UInt8",
    "Int8",
    "UInt16",
    "Int16",
    "UInt32",
    "Int32",
    "UInt64",
    "Int64",
    "Float32",
    "Float64",
    "Boolean",
    "String",
    "Bytes",
    "Timestamp",
    "Duration",
  ]) {
    it(`round trips ${type}`, async () => {
      const shape: Shape = { kind: "value", type };
      for (let seed = 0; seed < 4; seed++)
        await roundTrip(shape, sample(shape, seed));
    });
  }
  for (const name of Object.keys(LAYOUTS.records)) {
    it(`round trips every ${name} field with present and absent options`, async () => {
      const shape: Shape = { kind: "record", name };
      for (let seed = 0; seed < 4; seed++)
        await roundTrip(shape, sample(shape, seed));
    });
  }
  for (const [name, layout] of Object.entries(LAYOUTS.enums)) {
    for (let seed = 0; seed < Object.keys(layout.variants).length; seed++) {
      it(`round trips ${name} variant ${seed}`, async () => {
        const shape: Shape = { kind: "enum", name };
        await roundTrip(shape, sample(shape, seed));
      });
    }
  }
  for (const name of BRIDGED_OBJECTS) {
    it(`returns the live ${name} object through a handle`, async () => {
      const shape: Shape = { kind: "object", name };
      const original = sample(shape, 2);
      await roundTrip(shape, original);
    });
  }
  it("keeps foreign object handles live", async () => {
    for (const name of FOREIGN_OBJECTS) {
      const [main, worker] = endpoints();
      const host = new WorkerHost(
        worker,
        1,
        "foreign",
        async () => {},
        async () => undefined,
      );
      const session = new MainSession(main, 1, "foreign");
      await session.ready();
      const original = sample({ kind: "object", name }, 2);
      const handle = workerEncoder(host.registry).convert(
        { kind: "object", name },
        original,
      );
      expect(
        workerDecoder(host.registry, host.callbacks, enumFactory(B)).convert(
          { kind: "object", name },
          structuredClone(handle),
        ),
      ).toBe(original);
    }
  });
  it("round trips nested records, options, maps, lists, and live objects", async () => {
    const shape: Shape = { kind: "record", name: "BridgeProperty" };
    LAYOUTS.records.BridgeProperty = {
      fields: {
        object: { kind: "object", name: "Group" },
        option: { kind: "optional", inner: { kind: "object", name: "Group" } },
        list: { kind: "sequence", inner: { kind: "object", name: "Group" } },
        map: {
          kind: "map",
          key: { kind: "value", type: "String" },
          value: { kind: "object", name: "Group" },
        },
        count: { kind: "value", type: "UInt64" },
        bytes: { kind: "value", type: "Bytes" },
      },
    };
    const object = { marker: "live" };
    const original = {
      object,
      option: object,
      list: [object],
      map: new Map([["live", object]]),
      count: 9007199254740995n,
      bytes: new Uint8Array([0, 255]).buffer,
    };
    await roundTrip(shape, original);
    delete LAYOUTS.records.BridgeProperty;
  });
  it("detects a codec that drops a field", async () => {
    const shape: Shape = { kind: "record", name: "BackendOptions" };
    const original = sample(shape, 2);
    await expect(
      roundTrip(shape, original, (wire) => {
        if (wire && typeof wire === "object")
          Reflect.deleteProperty(wire, "url");
        return wire;
      }),
    ).rejects.toThrow();
  });
  it("detects a codec that drops credentials", async () => {
    const shape: Shape = { kind: "record", name: "BackendOptions" };
    const original = sample(shape, 2);
    await expect(
      roundTrip(shape, original, (wire) => {
        if (wire && typeof wire === "object")
          Reflect.deleteProperty(wire, "credentials");
        return wire;
      }),
    ).rejects.toThrow();
  });
  it("detects a spread codec that keeps an unknown field", async () => {
    const shape: Shape = { kind: "record", name: "BackendOptions" };
    const original = sample(shape, 2);
    await expect(
      roundTrip(shape, original, undefined, (value) => ({
        ...Object(value),
        leaked: "spread",
      })),
    ).rejects.toThrow();
  });

  it("keeps the Client owner open if end rejects", async () => {
    const [main, worker] = endpoints();
    let calls = 0;
    const host = new WorkerHost(
      worker,
      1,
      "end",
      async () => {},
      async () => {
        calls++;
        throw new Error("end failed");
      },
    );
    const session = new MainSession(main, 1, "end");
    await session.ready();
    const handle = host.registry.add({}, "Client");
    const client = new P.Client(session, handle);
    const ending = client.end();
    expect(() => session.checkHandle(handle)).toThrow("clientClosed");
    await expect(ending).rejects.toThrow("end failed");
    expect(() => session.checkHandle(handle)).not.toThrow();
    expect(
      main.sent.some(
        (message) =>
          message.t === "release" && message.owners?.includes(handle.owner),
      ),
    ).toBe(false);
    await expect(client.end()).rejects.toThrow("end failed");
    expect(calls).toBe(2);
    client.release();
  });

  it("reenters through generated foreign registration and stub", async () => {
    const [main, worker] = endpoints();
    let signed = false;
    const host = new WorkerHost(
      worker,
      1,
      "reentrant",
      async () => {},
      async (key, args, context) => {
        if (key === "inner") return "inner result";
        const raw = args[0];
        if (
          raw === null ||
          typeof raw !== "object" ||
          !("cb" in raw) ||
          typeof raw.cb !== "number"
        )
          throw new TypeError("missing signer callback");
        const stub = foreignStub(
          { cb: raw.cb, type: "Signer" },
          context.callbacks,
          context.registry,
        );
        const sign = Reflect.get(stub, "sign");
        const signature: unknown = await Reflect.apply(sign, stub, [
          { text: "sign me" },
        ]);
        signed = B.Signature.Ecdsa.instanceOf(signature);
        return "signed";
      },
    );
    const session = new MainSession(main, 1, "reentrant");
    await session.ready();
    class ReentrantSigner {
      async sign(): Promise<B.Signature> {
        expect(await session.call("inner", [])).toBe("inner result");
        return B.Signature.Ecdsa.new(new Uint8Array([1, 2, 3]).buffer);
      }
    }
    const callback = registerForeign("Signer", new ReentrantSigner(), session);
    expect(await session.call("outer", [callback])).toBe("signed");
    expect(signed).toBe(true);
    expect(host.registry.size).toBe(0);
  });
});
