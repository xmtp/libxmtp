import assert from "node:assert/strict";
import { Worker } from "node:worker_threads";

import {
  mainDecoder,
  mainEncoder,
} from "../../../../target/sdk-generated/typescript-wasm/codec.main.gen.ts";
import {
  CONTRACT_HASH,
  PROTOCOL_VERSION,
} from "../../../../target/sdk-generated/typescript-wasm/contract.gen.ts";
import * as P from "../../../../target/sdk-generated/typescript-wasm/proxy.gen.ts";
import {
  enumFactory,
  type Shape,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/codec.ts";
import { MainSession } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/session.ts";
import type {
  HandleWire,
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire.ts";
import { LAYOUTS } from "../../../../target/sdk-generated/typescript-wasm/wire.gen.ts";
import * as B from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk.ts";
import { PROPERTY_LAYOUT } from "./bridge.value-layout.mts";

LAYOUTS.records.BridgeProperty = PROPERTY_LAYOUT;
const worker = new Worker(new URL("./bridge.worker.mts", import.meta.url), {
  execArgv: process.execArgv,
});
const endpoint: WireEndpoint = {
  postMessage(message, transfer) {
    worker.postMessage(message, transfer);
  },
  onMessage(handler) {
    worker.on("message", (message: WireMessage) => handler(message));
  },
  onExit(handler) {
    worker.on("exit", handler);
    worker.on("error", handler);
  },
  terminate() {
    void worker.terminate();
  },
};
const session = new MainSession(endpoint, PROTOCOL_VERSION, CONTRACT_HASH);
const handles = new Map<string, HandleWire>();

async function sampleWire(shape: Shape, seed: number): Promise<unknown> {
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
    case "custom":
      return sampleWire(shape.inner, seed);
    case "object": {
      const existing = handles.get(shape.name);
      if (existing) return existing;
      const value = await session.call("__bridgeValueObject", [shape.name]);
      if (
        value === null ||
        typeof value !== "object" ||
        !("h" in value) ||
        typeof value.h !== "number" ||
        !("owner" in value) ||
        typeof value.owner !== "number" ||
        !("epoch" in value) ||
        typeof value.epoch !== "number" ||
        !("type" in value) ||
        value.type !== shape.name
      )
        throw new TypeError("invalid test object handle");
      const handle: HandleWire = {
        h: value.h,
        owner: value.owner,
        epoch: value.epoch,
        type: value.type,
      };
      handles.set(shape.name, handle);
      return handle;
    }
    case "record": {
      const fields = LAYOUTS.records[shape.name]?.fields;
      if (!fields) throw new TypeError(`unknown record ${shape.name}`);
      const entries: Array<[string, unknown]> = [];
      for (const [index, [name, field]] of Object.entries(fields).entries()) {
        if (shape.name === "ClientOptions" && name === "backend") continue;
        entries.push([name, await sampleWire(field, seed + index)]);
      }
      if (shape.name === "ClientOptions") {
        const backend = await sampleWire(
          { kind: "object", name: "Backend" },
          seed,
        );
        return { ...Object.fromEntries(entries), backend: { tag: "Connected", inner: { backend } } };
      }
      return Object.fromEntries(entries);
    }
    case "enum": {
      const layout = LAYOUTS.enums[shape.name];
      if (!layout) throw new TypeError(`unknown enum ${shape.name}`);
      const variants = Object.entries(layout.variants);
      const entry = variants[seed % variants.length];
      if (!entry) throw new TypeError(`empty enum ${shape.name}`);
      if (layout.flat) return seed % variants.length;
      const [tag, fields] = entry;
      if (Array.isArray(fields)) {
        const inner: unknown[] = [];
        for (const [index, field] of fields.entries())
          inner.push(await sampleWire(field, seed + index));
        if (layout.error)
          return {
            variant: tag,
            code: `sample-${seed}`,
            category: 7,
            retryable: false,
            message: `sample-${seed}`,
            details: inner,
          };
        return inner.length === 0 ? { tag } : { tag, inner };
      }
      if (!fields) throw new TypeError("missing enum fields");
      const entries: Array<[string, unknown]> = [];
      for (const [index, [name, field]] of Object.entries(fields).entries())
        entries.push([name, await sampleWire(field, seed + index)]);
      const inner = Object.fromEntries(entries);
      return Object.keys(inner).length === 0 ? { tag } : { tag, inner };
    }
    case "optional":
      if (shape.inner.kind === "foreign" || shape.inner.kind === "callback")
        return undefined;
      return seed % 2 === 0 ? sampleWire(shape.inner, seed) : undefined;
    case "sequence":
      return [
        await sampleWire(shape.inner, seed),
        await sampleWire(shape.inner, seed + 2),
      ];
    case "set":
      return new Set([await sampleWire(shape.inner, seed)]);
    case "map":
      return new Map([
        [
          await sampleWire(shape.key, seed),
          await sampleWire(shape.value, seed),
        ],
      ]);
    case "foreign":
    case "callback":
      throw new TypeError("foreign callback is not a worker output");
  }
}

function decodeMain(shape: Shape, wire: unknown): unknown {
  if (
    (shape.kind === "record" ||
      shape.kind === "enum" ||
      shape.kind === "object") &&
    shape.name !== "BridgeProperty"
  ) {
    const decoder = Reflect.get(
      P,
      `decode${shape.kind[0].toUpperCase()}${shape.kind.slice(1)}${shape.name}`,
    );
    assert.equal(typeof decoder, "function", `missing ${shape.name} decoder`);
    return Reflect.apply(decoder, undefined, [session, wire]);
  }
  return mainDecoder(session, enumFactory(B), (handle) =>
    P.proxyFor(session, handle),
  ).convert(shape, wire);
}

const cases: Array<{ name: string; shape: Shape; seed: number }> = [
  {
    name: "record with nested fields",
    shape: { kind: "record", name: "MessageData" },
    seed: 2,
  },
  { name: "flat enum", shape: { kind: "enum", name: "MessageKind" }, seed: 1 },
  {
    name: "tagged tuple enum",
    shape: { kind: "enum", name: "MessageContent" },
    seed: 0,
  },
  {
    name: "tagged record enum",
    shape: { kind: "enum", name: "Signature" },
    seed: 1,
  },
  {
    name: "error enum value",
    shape: { kind: "enum", name: "XmtpError" },
    seed: 0,
  },
  { name: "bytes", shape: { kind: "value", type: "Bytes" }, seed: 2 },
  { name: "large bigint", shape: { kind: "value", type: "UInt64" }, seed: 2 },
  {
    name: "present optional record",
    shape: {
      kind: "optional",
      inner: { kind: "record", name: "PublicIdentity" },
    },
    seed: 2,
  },
  {
    name: "absent optional record",
    shape: {
      kind: "optional",
      inner: { kind: "record", name: "PublicIdentity" },
    },
    seed: 1,
  },
  {
    name: "list of records",
    shape: { kind: "sequence", inner: { kind: "record", name: "Credential" } },
    seed: 2,
  },
  {
    name: "map of records",
    shape: {
      kind: "map",
      key: { kind: "value", type: "String" },
      value: { kind: "record", name: "Credential" },
    },
    seed: 2,
  },
  {
    name: "set of bigint",
    shape: { kind: "set", inner: { kind: "value", type: "UInt64" } },
    seed: 2,
  },
  { name: "live object", shape: { kind: "object", name: "Group" }, seed: 2 },
  {
    name: "present optional live object",
    shape: { kind: "optional", inner: { kind: "object", name: "Group" } },
    seed: 2,
  },
  {
    name: "list of live objects",
    shape: { kind: "sequence", inner: { kind: "object", name: "Group" } },
    seed: 2,
  },
  {
    name: "map of live objects",
    shape: {
      kind: "map",
      key: { kind: "value", type: "String" },
      value: { kind: "object", name: "Group" },
    },
    seed: 2,
  },
  {
    name: "live objects inside a record",
    shape: { kind: "record", name: "BridgeProperty" },
    seed: 1,
  },
  {
    name: "BackendSource.Connected object inside ClientOptions",
    shape: { kind: "record", name: "ClientOptions" },
    seed: 2,
  },
];

try {
  await session.ready();
  for (const { name, shape, seed } of cases) {
    const raw = await sampleWire(shape, seed);
    const result = await session.call("__bridgeValueBegin", [shape, raw]);
    if (
      result === null ||
      typeof result !== "object" ||
      !("token" in result) ||
      typeof result.token !== "number" ||
      !("wire" in result)
    )
      throw new TypeError(`invalid ${name} result`);
    const decoded = decodeMain(shape, result.wire);
    const returned = mainEncoder(session).convert(shape, decoded);
    assert.deepEqual(
      await session.call("__bridgeValueCheck", [result.token, returned]),
      { sameBytes: true, sameIdentity: true },
      `${name} changed across the real worker`,
    );
  }
  console.log(
    `real WASM worker round trips passed for ${cases.length} value kinds`,
  );
} finally {
  await worker.terminate();
}
