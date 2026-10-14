import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { serialize } from "node:v8";

import { describe, expect, it } from "vitest";

import { mainDecoder, mainEncoder } from "./codec.main.gen.js";
import { workerDecoder, workerEncoder } from "./codec.worker.gen.js";
import { dispatchGenerated } from "./dispatch.gen.js";
import * as P from "./proxy.gen.js";
import { registerForeign } from "./reverse.gen.js";
import { enumFactory, type Shape } from "./runtime/bridge/codec.js";
import { RemoteObject } from "./runtime/bridge/main/remote-object.js";
import { MainSession } from "./runtime/bridge/main/session.js";
import type { WireEndpoint, WireMessage } from "./runtime/bridge/wire.js";
import {
  PoolLocks,
  RUST_PANIC_PREFIX,
  WorkerHost,
  type LockProvider,
} from "./runtime/bridge/worker/host.js";
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

const SAMPLE_DEPTH = 4;

// Containers can end a recursive value. Required fields need a finite path.
function minimumSampleDepth(shape: Shape, seen = new Set<string>()): number {
  switch (shape.kind) {
    case "value":
    case "object":
    case "foreign":
    case "callback":
    case "optional":
    case "sequence":
    case "set":
    case "map":
      return 0;
    case "custom":
      return 1 + minimumSampleDepth(shape.inner, seen);
    case "record":
    case "enum": {
      const key = `${shape.kind}:${shape.name}`;
      if (seen.has(key)) return Infinity;
      const next = new Set(seen).add(key);
      const fieldsDepth = (fields: Record<string, Shape> | Shape[]): number =>
        Math.max(
          0,
          ...Object.values(fields).map((field) =>
            minimumSampleDepth(field, next),
          ),
        );
      if (shape.kind === "record") {
        const layout = LAYOUTS.records[shape.name];
        if (!layout) throw new Error(`unknown record ${shape.name}`);
        return 1 + fieldsDepth(layout.fields);
      }
      const layout = LAYOUTS.enums[shape.name];
      if (!layout) throw new Error(`unknown enum ${shape.name}`);
      if (layout.flat) return 0;
      return (
        1 +
        Math.min(
          ...Object.values(layout.variants).map((fields) =>
            fieldsDepth(fields!),
          ),
        )
      );
    }
  }
}

function sample(shape: Shape, seed: number, depth = 0): unknown {
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
    case "custom": {
      const inner = sample(shape.inner, seed, depth + 1);
      const value = enumFactory(B).custom?.(shape.name, inner);
      if (value === undefined)
        throw new Error(`missing ${shape.name} custom factory`);
      return value;
    }
    case "object":
    case "foreign":
    case "callback":
      return { marker: shape.name, seed };
    case "record": {
      const layout = LAYOUTS.records[shape.name];
      if (!layout) throw new Error(`unknown record ${shape.name}`);
      if (depth >= SAMPLE_DEPTH && !Number.isFinite(minimumSampleDepth(shape)))
        throw new Error(`no finite sample for record ${shape.name}`);
      const result: Record<string, unknown> = {};
      for (const [index, [name, field]] of Object.entries(
        layout.fields,
      ).entries())
        result[name] = sample(field, seed + index, depth + 1);
      return result;
    }
    case "enum": {
      const layout = LAYOUTS.enums[shape.name];
      if (!layout) throw new Error(`unknown enum ${shape.name}`);
      const variants = Object.entries(layout.variants);
      let entry = variants[seed % variants.length];
      if (depth >= SAMPLE_DEPTH && !layout.flat) {
        const seen = new Set([`enum:${shape.name}`]);
        const ranked = variants.map((candidate) => ({
          candidate,
          depth: Math.max(
            0,
            ...Object.values(candidate[1]!).map((field) =>
              minimumSampleDepth(field, seen),
            ),
          ),
        }));
        ranked.sort((a, b) => a.depth - b.depth);
        if (!ranked[0] || !Number.isFinite(ranked[0].depth))
          throw new Error(`no finite sample for enum ${shape.name}`);
        entry = ranked[0].candidate;
      }
      if (!entry) throw new Error(`empty enum ${shape.name}`);
      if (layout.flat) return seed % variants.length;
      return enumSample(shape.name, entry[0], entry[1]!, seed, depth);
    }
    case "optional":
      if (shape.inner.kind === "foreign" || shape.inner.kind === "callback")
        return undefined;
      return depth < SAMPLE_DEPTH && seed % 2 === 0
        ? sample(shape.inner, seed, depth + 1)
        : undefined;
    case "sequence":
      return depth >= SAMPLE_DEPTH
        ? []
        : [
            sample(shape.inner, seed, depth + 1),
            sample(shape.inner, seed + 2, depth + 1),
          ];
    case "set":
      return new Set(
        depth >= SAMPLE_DEPTH ? [] : [sample(shape.inner, seed, depth + 1)],
      );
    case "map":
      return new Map(
        depth >= SAMPLE_DEPTH
          ? []
          : [
              [
                sample(shape.key, seed, depth + 1),
                sample(shape.value, seed, depth + 1),
              ],
            ],
      );
  }
}

function enumSample(
  name: string,
  tag: string,
  fields: Record<string, Shape> | Shape[],
  seed: number,
  depth: number,
): unknown {
  if (Array.isArray(fields))
    return enumFactory(B)(
      name,
      tag,
      fields.map((field) => sample(field, seed, depth + 1)),
    );
  if (Object.keys(fields).length === 0) return enumFactory(B)(name, tag, []);
  const inner: Record<string, unknown> = {};
  for (const [field, shape] of Object.entries(fields))
    inner[field] = sample(shape, seed, depth + 1);
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
    case "custom":
      return containsForeign(shape.inner, value);
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
  it("samples recursive records with nonempty options and collections", () => {
    const shape: Shape = { kind: "record", name: "RecursiveSample" };
    LAYOUTS.records.RecursiveSample = {
      fields: {
        label: { kind: "value", type: "String" },
        children: { kind: "sequence", inner: shape },
        option: { kind: "optional", inner: shape },
        set: { kind: "set", inner: shape },
        map: {
          kind: "map",
          key: { kind: "value", type: "String" },
          value: shape,
        },
      },
    };
    type RecursiveSample = {
      label: string;
      children: RecursiveSample[];
      option?: RecursiveSample;
      set: Set<RecursiveSample>;
      map: Map<string, RecursiveSample>;
    };
    function isRecursiveSample(value: unknown): value is RecursiveSample {
      if (value === null || typeof value !== "object") return false;
      const label = Reflect.get(value, "label");
      const children = Reflect.get(value, "children");
      const option = Reflect.get(value, "option");
      const set = Reflect.get(value, "set");
      const map = Reflect.get(value, "map");
      return (
        typeof label === "string" &&
        Array.isArray(children) &&
        children.every(isRecursiveSample) &&
        (option === undefined || isRecursiveSample(option)) &&
        set instanceof Set &&
        Array.from(set).every(isRecursiveSample) &&
        map instanceof Map &&
        Array.from(map).every(
          ([key, item]) => typeof key === "string" && isRecursiveSample(item),
        )
      );
    }
    try {
      const value = sample(shape, 0);
      if (!isRecursiveSample(value))
        throw new Error("invalid recursive sample");
      expect(value.label).toBe("sample-0");
      expect(value.children).toHaveLength(2);
      expect(value.children[0].children).toHaveLength(2);
      expect(value.children[0].children[0].children).toEqual([]);
      expect(value.option?.label).toBe("sample-2");
      expect(value.set.size).toBe(1);
      expect(value.map.get("sample-4")?.label).toBe("sample-4");
    } finally {
      delete LAYOUTS.records.RecursiveSample;
    }
  });
  it("reports a recursive record with no finite sample", () => {
    const shape: Shape = { kind: "record", name: "RequiredRecursiveSample" };
    LAYOUTS.records.RequiredRecursiveSample = { fields: { child: shape } };
    try {
      expect(() => sample(shape, 0)).toThrow(
        "no finite sample for record RequiredRecursiveSample",
      );
    } finally {
      delete LAYOUTS.records.RequiredRecursiveSample;
    }
  });
  it("finds a finite path through required recursive enums", () => {
    const shape: Shape = { kind: "enum", name: "RecursiveEnumSample" };
    LAYOUTS.enums.RecursiveEnumSample = {
      flat: false,
      error: false,
      variants: { Recur: [shape], Leaf: [{ kind: "value", type: "String" }] },
    };
    try {
      expect(minimumSampleDepth(shape)).toBe(1);
      expect(
        minimumSampleDepth(shape, new Set(["enum:RecursiveEnumSample"])),
      ).toBe(Infinity);
    } finally {
      delete LAYOUTS.enums.RecursiveEnumSample;
    }
  });

  it("matches the pinned WASM panic fallback prefix", () => {
    const source = readFileSync(
      new URL(
        "./node_modules/@ubjs/wasm/dist/core/src/module.js",
        import.meta.url,
      ),
      "utf8",
    );
    expect(source).toContain(`console.error("${RUST_PANIC_PREFIX} `);
  });

  it("loads the real WASM bridge in worker_threads", () => {
    const output = execFileSync(
      "sdks/node/node_modules/.bin/tsx",
      ["sdks/browser/test/platform/bridge.real.mts"],
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

  it("round trips value kinds through the initialized WASM worker", () => {
    const output = execFileSync(
      "sdks/node/node_modules/.bin/tsx",
      ["sdks/browser/test/platform/bridge.values.real.mts"],
      {
        cwd: process.cwd(),
        env: { ...process.env, NODE_OPTIONS: "--preserve-symlinks" },
        encoding: "utf8",
        timeout: 120000,
      },
    );
    expect(output).toContain("real WASM worker round trips passed");
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
    const [main, worker] = endpoints();
    const host = new WorkerHost(
      worker,
      1,
      "credentials",
      async () => {},
      async () => undefined,
    );
    const session = new MainSession(main, 1, "credentials");
    await session.ready();
    const original = {
      url: "https://example.test",
      appVersion: undefined,
      credential: undefined,
      credentials: {
        async credential() {
          return {
            name: undefined,
            value: "token",
            expiresAtSeconds: 9007199254740993n,
          };
        },
      },
    };
    const wire = mainEncoder(session).convert(shape, original);
    const invoke = async (value: unknown): Promise<unknown> => {
      const decoded = workerDecoder(
        host.registry,
        host.callbacks,
        enumFactory(B),
      ).convert(shape, value);
      if (decoded === null || typeof decoded !== "object")
        throw new TypeError("missing options");
      const source: unknown = Reflect.get(decoded, "credentials");
      if (source === null || typeof source !== "object")
        throw new TypeError("missing credentials");
      const callback: unknown = Reflect.get(source, "credential");
      if (typeof callback !== "function")
        throw new TypeError("missing credential callback");
      return Reflect.apply(callback, source, []);
    };
    expect(await invoke(structuredClone(wire))).toMatchObject({
      value: "token",
    });
    const dropped = structuredClone(wire);
    if (dropped !== null && typeof dropped === "object")
      Reflect.deleteProperty(dropped, "credentials");
    await expect(invoke(dropped)).rejects.toThrow("missing credentials");
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
    const handle = host.registry.add({}, "Client", undefined, () => ({
      clientKey: 1n,
    }));
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

  it("closes the Client owner only through the generated end", async () => {
    const [main, worker] = endpoints();
    let ends = 0;
    const host = new WorkerHost(
      worker,
      1,
      "owner",
      async () => {},
      async (key) => {
        if (key === "Client.end") ends++;
      },
    );
    const session = new MainSession(main, 1, "owner");
    await session.ready();
    const handle = host.registry.add({}, "Client", undefined, () => ({
      clientKey: 1n,
    }));
    const client = new P.Client(session, handle);
    const keys: string[] = [];
    for (
      let item: object | null = client;
      item;
      item = Object.getPrototypeOf(item)
    )
      keys.push(...Object.getOwnPropertyNames(item));
    expect(keys).not.toContain("endOwner");
    expect("endOwner" in client).toBe(false);
    await client.end();
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    expect(ends).toBe(1);
    expect(
      main.sent.some(
        (message) =>
          message.t === "release" && message.owners?.includes(handle.owner),
      ),
    ).toBe(true);
    expect(host.registry.size).toBe(0);
    expect(() => session.checkHandle(handle)).toThrow("clientClosed");
  });

  it("rejects a generated call on a handle of another type", async () => {
    const held = new Set<string>();
    const provider: LockProvider = {
      async request(name, _options, callback) {
        if (held.has(name)) return callback(null);
        held.add(name);
        try {
          await callback({});
        } finally {
          held.delete(name);
        }
      },
    };
    const locks = new PoolLocks(provider);
    const otherTab = new PoolLocks(provider);
    const [main, worker] = endpoints();
    const host = new WorkerHost(
      worker,
      1,
      "target",
      async () => {},
      dispatchGenerated,
      locks,
    );
    const session = new MainSession(main, 1, "target");
    await session.ready();
    const start = async (): Promise<{
      client: P.Client;
      reader: { h: number };
      ends: { client: number; reader: number; lockHeld: boolean[] };
    }> => {
      const lockHeld: boolean[] = [];
      const ends = { client: 0, reader: 0, lockHeld };
      await locks.open("client-pool");
      const clientHandle = host.registry.add(
        {
          end: async () => {
            ends.client++;
            ends.lockHeld.push(held.has("xmtp:client-pool"));
          },
        },
        "Client",
        undefined,
        () => ({ clientKey: 1n }),
      );
      locks.attachOwner(clientHandle.owner, "client-pool");
      const reader = host.registry.add(
        {
          end: async () => {
            ends.reader++;
          },
        },
        "MessageReader",
        clientHandle.owner,
      );
      await expect(
        session.call("Client.end", [], reader),
      ).rejects.toMatchObject({ code: "ContractMismatch" });
      await expect(
        session.call("MessageReader.end", [], clientHandle),
      ).rejects.toMatchObject({ code: "ContractMismatch" });
      await expect(
        session.call("Client.create", [], clientHandle),
      ).rejects.toMatchObject({ code: "ContractMismatch" });
      expect(ends).toMatchObject({ client: 0, reader: 0 });
      return { client: new P.Client(session, clientHandle), reader, ends };
    };

    // An owner release ends the client before it releases the pool lock.
    const released = await start();
    main.postMessage({
      t: "release",
      handles: [released.client.handle.h, released.reader.h],
      owners: [released.client.handle.owner],
    });
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    expect(released.ends).toEqual({ client: 1, reader: 0, lockHeld: [true] });
    await otherTab.open("client-pool");
    otherTab.close("client-pool");
    await new Promise<void>((resolve) => setTimeout(resolve, 0));

    // A generated Client.end ends the client once and releases the lock.
    const ended = await start();
    await ended.client.end();
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    expect(ended.ends).toEqual({ client: 1, reader: 0, lockHeld: [true] });
    expect(host.registry.size).toBe(0);
    await otherTab.open("client-pool");
    otherTab.close("client-pool");
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

  it("dispatches only generated callback methods to a foreign object", async () => {
    const [main, worker] = endpoints();
    let cb = 0;
    new WorkerHost(
      worker,
      1,
      "declared",
      async () => {},
      async (key, _args, context) => context.callbacks.invoke(cb, key, []),
    );
    const session = new MainSession(main, 1, "declared");
    await session.ready();
    let exposed = 0;
    class AppSigner {
      async identity(): Promise<B.PublicIdentity> {
        return { identifier: "0x01", kind: B.PublicIdentityKind.Ethereum };
      }
      getPrivateKey(): string {
        exposed++;
        return "secret";
      }
    }
    cb = registerForeign("Signer", new AppSigner(), session).cb;
    for (const method of ["getPrivateKey", "constructor", "toString"])
      await expect(session.call(method, [])).rejects.toMatchObject({
        code: "ContractMismatch",
      });
    expect(exposed).toBe(0);
    await expect(session.call("identity", [])).resolves.toMatchObject({
      identifier: "0x01",
    });
  });

  it("drops the callbacks of a generated call that is never sent", async () => {
    const signer = {
      async identity(): Promise<B.PublicIdentity> {
        return { identifier: "0x01", kind: B.PublicIdentityKind.Ethereum };
      },
      async kind(): Promise<B.SignerKind> {
        return B.SignerKind.Eoa.new();
      },
      async sign(): Promise<B.Signature> {
        throw new Error("not signed");
      },
    };
    const registered = (session: MainSession): number => {
      const targets: unknown = Reflect.get(session.callbacks, "targets");
      if (!(targets instanceof Map)) throw new TypeError("missing targets");
      return targets.size;
    };

    // A later argument fails to encode after the signer was registered.
    const [main, worker] = endpoints();
    const host = new WorkerHost(
      worker,
      1,
      "unsent",
      async () => {},
      async () => undefined,
    );
    const session = new MainSession(main, 1, "unsent");
    await session.ready();
    const handle = host.registry.add({}, "Client", undefined, () => ({
      clientKey: 1n,
    }));
    const client = new P.Client(session, handle);
    await expect(
      Reflect.apply(client.removeAccount, client, [signer, null]),
    ).rejects.toThrow("expected bridge record");
    expect(registered(session)).toBe(0);

    // The endpoint rejects the message.
    const post = main.postMessage.bind(main);
    main.postMessage = (message) => {
      if (message.t === "call") throw new Error("post failed");
      post(message);
    };
    await expect(client.revokeAllOtherInstallations(signer)).rejects.toThrow(
      "post failed",
    );
    expect(registered(session)).toBe(0);
    client.release();

    // The call is aborted before the worker handshake.
    const waiting = new MainSession(new Endpoint(), 1, "waiting");
    const early = new P.Client(waiting, {
      h: 1,
      owner: 1,
      epoch: 0,
      type: "Client",
      snap: { clientKey: 1n },
    });
    const abort = new AbortController();
    const call = early.revokeAllOtherInstallations(signer, {
      signal: abort.signal,
    });
    abort.abort();
    await expect(call).rejects.toMatchObject({ code: "Cancelled" });
    expect(registered(waiting)).toBe(0);
  });
});
