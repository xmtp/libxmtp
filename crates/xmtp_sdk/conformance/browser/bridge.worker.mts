import { serialize } from "node:v8";
import { parentPort } from "node:worker_threads";

import {
  workerDecoder,
  workerEncoder,
} from "../../../../target/sdk-generated/typescript-wasm/codec.worker.gen.ts";
import {
  CONTRACT_HASH,
  PROTOCOL_VERSION,
} from "../../../../target/sdk-generated/typescript-wasm/contract.gen.ts";
import { dispatchGenerated } from "../../../../target/sdk-generated/typescript-wasm/dispatch.gen.ts";
import { uniffiInitAsync } from "../../../../target/sdk-generated/typescript-wasm/index.ts";
import {
  enumFactory,
  type Shape,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/codec.ts";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire.ts";
import {
  PoolLocks,
  WorkerHost,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/worker/host.ts";
import { LAYOUTS } from "../../../../target/sdk-generated/typescript-wasm/wire.gen.ts";
import * as B from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk.ts";
import { PROPERTY_LAYOUT } from "./bridge.value-layout.mts";

LAYOUTS.records.BridgeProperty = PROPERTY_LAYOUT;
const originals = new Map<number, { shape: Shape; value: unknown }>();
let nextValue = 1;

function semantic(value: unknown): unknown {
  return value instanceof Error && "tag" in value
    ? { tag: value.tag, inner: "inner" in value ? value.inner : undefined }
    : value;
}

function sameIdentity(
  shape: Shape,
  original: unknown,
  restored: unknown,
): boolean {
  if (shape.kind === "object") return original === restored;
  if (shape.kind !== "record" || shape.name !== "BridgeProperty") return true;
  if (
    original === null ||
    restored === null ||
    typeof original !== "object" ||
    typeof restored !== "object"
  )
    return false;
  const object = Reflect.get(original, "object");
  const list = Reflect.get(restored, "list");
  const map = Reflect.get(restored, "map");
  return (
    Reflect.get(restored, "object") === object &&
    Reflect.get(restored, "option") === Reflect.get(original, "option") &&
    Array.isArray(list) &&
    list[0] === object &&
    map instanceof Map &&
    [...map.values()][0] === object
  );
}

if (!parentPort) throw new Error("bridge worker has no parent port");
const port = parentPort;
const endpoint: WireEndpoint = {
  postMessage(message, transfer) {
    port.postMessage(message, transfer);
  },
  onMessage(handler) {
    port.on("message", (message: WireMessage) => handler(message));
  },
  onExit(handler) {
    port.on("close", handler);
  },
  close() {
    port.close();
  },
};

const wasm = new URL(
  "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk.wasm",
  import.meta.url,
);
const host = new WorkerHost(
  endpoint,
  PROTOCOL_VERSION,
  CONTRACT_HASH,
  async () => {
    await uniffiInitAsync(wasm);
  },
  async (key, args, context) => {
    if (key === "__bridgeInner") return "inner result";
    if (key === "__bridgeNever") return new Promise<unknown>(() => {});
    if (key === "__bridgeValueObject") {
      const name = args[0];
      if (typeof name !== "string") throw new TypeError("missing object type");
      return context.registry.add({ marker: name }, name);
    }
    if (key === "__bridgeValueBegin") {
      const shape = args[0] as Shape;
      const original = workerDecoder(
        context.registry,
        context.callbacks,
        enumFactory(B),
      ).convert(shape, args[1]);
      const token = nextValue++;
      originals.set(token, { shape, value: original });
      return {
        token,
        wire: workerEncoder(context.registry).convert(shape, original),
      };
    }
    if (key === "__bridgeValueCheck") {
      const token = args[0];
      if (typeof token !== "number") throw new TypeError("missing value token");
      const entry = originals.get(token);
      if (!entry) throw new TypeError("unknown value token");
      originals.delete(token);
      const restored = workerDecoder(
        context.registry,
        context.callbacks,
        enumFactory(B),
      ).convert(entry.shape, args[1]);
      return {
        sameBytes:
          Buffer.compare(
            serialize(semantic(entry.value)),
            serialize(semantic(restored)),
          ) === 0,
        sameIdentity: sameIdentity(entry.shape, entry.value, restored),
      };
    }
    if (key === "__bridgeReentrantSigner") {
      const callback = args[0];
      if (
        callback === null ||
        typeof callback !== "object" ||
        !("cb" in callback) ||
        typeof callback.cb !== "number"
      ) {
        throw new TypeError("invalid signer callback");
      }
      return context.callbacks.invoke(callback.cb, "sign", []);
    }
    return dispatchGenerated(key, args, context);
  },
  new PoolLocks({
    async request(_name, _options, callback) {
      await callback({});
    },
  }),
);
process.on("unhandledRejection", (error) => host.fatal(error));
process.on("uncaughtException", (error) => host.fatal(error));
