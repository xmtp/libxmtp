import { serialize } from "node:v8";

import { describe, expect, it } from "vitest";

import { mainEncoder } from "./codec.main.gen.js";
import { workerDecoder } from "./codec.worker.gen.js";
import { ValueCodec, type Shape } from "./runtime/bridge/codec.js";
import { MainSession } from "./runtime/bridge/main/session.js";
import {
  BridgeError,
  type WireEndpoint,
  type WireMessage,
} from "./runtime/bridge/wire.js";
import { WorkerHost } from "./runtime/bridge/worker/host.js";
import { WorkerRegistry } from "./runtime/bridge/worker/registry.js";
import { BRIDGED_OBJECTS, FOREIGN_OBJECTS, LAYOUTS } from "./wire.gen.js";

class Endpoint implements WireEndpoint {
  peer?: Endpoint;
  private receive: (message: WireMessage) => void = () => {};
  private exitHandler: () => void = () => {};

  postMessage(message: WireMessage): void {
    const clone = structuredClone(message);
    queueMicrotask(() => this.peer?.receive(clone));
  }
  onMessage(handler: (message: WireMessage) => void): void {
    this.receive = handler;
  }
  onExit(handler: () => void): void {
    this.exitHandler = handler;
  }
}

function endpoints(): [Endpoint, Endpoint] {
  const main = new Endpoint();
  const worker = new Endpoint();
  main.peer = worker;
  worker.peer = main;
  return [main, worker];
}

function sample(shape: Shape): unknown {
  switch (shape.kind) {
    case "value":
      switch (shape.type) {
        case "Boolean":
          return true;
        case "String":
          return "sample";
        case "Bytes":
          return new Uint8Array([0, 1, 255]);
        case "UInt64":
        case "Int64":
          return 9007199254740993n;
        case "Timestamp":
          return new Date(1700000000000);
        case "Duration":
          return 1000;
        case undefined:
          return undefined;
        default:
          return 17;
      }
    case "object":
    case "foreign":
    case "callback":
      return { marker: shape.name };
    case "record": {
      const layout = LAYOUTS.records[shape.name];
      if (!layout) throw new Error(`unknown record ${shape.name}`);
      return Object.fromEntries(
        Object.entries(layout.fields).map(([name, field]) => [
          name,
          sample(field),
        ]),
      );
    }
    case "enum": {
      const layout = LAYOUTS.enums[shape.name];
      if (!layout) throw new Error(`unknown enum ${shape.name}`);
      if (layout.error)
        return new BridgeError("Test", "test", "unknown", false, "test", {
          value: 1,
        });
      const first = Object.entries(layout.variants)[0];
      if (!first) throw new Error(`empty enum ${shape.name}`);
      return enumSample(first[0], first[1]);
    }
    case "optional":
      return undefined;
    case "sequence":
      return [sample(shape.inner)];
    case "set":
      return new Set([sample(shape.inner)]);
    case "map":
      return new Map([[sample(shape.key), sample(shape.value)]]);
  }
}

function enumSample(
  tag: string,
  fields: Record<string, Shape> | Shape[],
): unknown {
  if (Array.isArray(fields)) return { tag, inner: fields.map(sample) };
  const entries = Object.entries(fields);
  return entries.length === 0
    ? { tag }
    : {
        tag,
        inner: Object.fromEntries(
          entries.map(([name, shape]) => [name, sample(shape)]),
        ),
      };
}

function roundTrip(shape: Shape, value: unknown): void {
  const registry = new WorkerRegistry(1);
  const workerOut = new ValueCodec(
    LAYOUTS,
    "worker",
    "encode",
    undefined,
    registry,
  );
  const workerIn = new ValueCodec(
    LAYOUTS,
    "worker",
    "decode",
    undefined,
    registry,
  );
  const mainOut = new ValueCodec(LAYOUTS, "main", "encode");
  const mainIn = new ValueCodec(LAYOUTS, "main", "decode");
  const wire = workerOut.convert(shape, value);
  const mainValue = mainIn.convert(shape, structuredClone(wire));
  const returned = mainOut.convert(shape, mainValue);
  const workerValue = workerIn.convert(shape, structuredClone(returned));
  expect(serialize(workerOut.convert(shape, workerValue))).toEqual(
    serialize(wire),
  );
}

describe("generated bridge values", () => {
  it("round trips a reentrant signer through generated stubs", async () => {
    const [main, worker] = endpoints();
    const host = new WorkerHost(
      worker,
      1,
      "signer",
      async () => {},
      async (key) => key,
    );
    const session = new MainSession(main, 1, "signer");
    await session.ready();
    const shape: Shape = { kind: "foreign", name: "Signer" };
    const handle = mainEncoder(session).convert(shape, {
      async identity() {
        return sample({ kind: "record", name: "PublicIdentity" });
      },
      async kind() {
        return { tag: "Eoa" };
      },
      async sign(request: unknown) {
        expect(request).toMatchObject({ text: "sample" });
        expect(await session.call("inside", [])).toBe("inside");
        return { tag: "Ecdsa", inner: [new Uint8Array([1, 2, 3])] };
      },
    });
    const signer = workerDecoder(
      host.registry,
      host.callbacks,
      (_name, tag, fields) => ({ tag, inner: fields }),
    ).convert(shape, structuredClone(handle));
    const sign: unknown = Reflect.get(signer, "sign");
    if (typeof sign !== "function") throw new TypeError("sign stub missing");
    const signature: unknown = await Reflect.apply(sign, signer, [
      { text: "sample" },
    ]);
    expect(signature).toMatchObject({
      tag: "Ecdsa",
      inner: [new Uint8Array([1, 2, 3])],
    });
  });
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
    it(`round trips scalar ${type}`, () => {
      const shape: Shape = { kind: "value", type };
      roundTrip(shape, sample(shape));
    });
  }
  for (const name of Object.keys(LAYOUTS.records)) {
    it(`round trips record ${name}`, () => {
      const shape: Shape = { kind: "record", name };
      roundTrip(shape, sample(shape));
    });
  }
  for (const [name, layout] of Object.entries(LAYOUTS.enums)) {
    if (layout.error) {
      it(`round trips error ${name}`, () => {
        const shape: Shape = { kind: "enum", name };
        roundTrip(shape, sample(shape));
      });
    } else {
      for (const [tag, fields] of Object.entries(layout.variants)) {
        it(`round trips ${name}.${tag}`, () => {
          roundTrip({ kind: "enum", name }, enumSample(tag, fields));
        });
      }
    }
  }
  for (const name of [...BRIDGED_OBJECTS, ...FOREIGN_OBJECTS]) {
    it(`keeps ${name} behind its handle`, () => {
      const registry = new WorkerRegistry(1);
      const value = { id: 1 };
      const out = new ValueCodec(
        LAYOUTS,
        "worker",
        "encode",
        undefined,
        registry,
      );
      const back = new ValueCodec(
        LAYOUTS,
        "worker",
        "decode",
        undefined,
        registry,
      );
      const shape: Shape = { kind: "object", name };
      expect(
        back.convert(shape, structuredClone(out.convert(shape, value))),
      ).toBe(value);
    });
  }
});
