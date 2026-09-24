import { ValueCodec, type EnumFactory } from "./runtime/bridge/codec.js";
import type { WorkerCallbacks } from "./runtime/bridge/worker/callback-stub.js";
import type { WorkerRegistry } from "./runtime/bridge/worker/registry.js";
import { foreignStub } from "./stubs.gen.js";
import { LAYOUTS } from "./wire.gen.js";

export function workerEncoder(
  registry: WorkerRegistry,
  owner?: number,
  snapshot?: (name: string, value: object, owner: number) => unknown,
): ValueCodec {
  return new ValueCodec(
    LAYOUTS,
    "worker",
    "encode",
    undefined,
    registry,
    undefined,
    undefined,
    undefined,
    owner,
    undefined,
    snapshot,
  );
}

export function workerDecoder(
  registry: WorkerRegistry,
  callbacks: WorkerCallbacks,
  enumFactory: EnumFactory,
): ValueCodec {
  return new ValueCodec(
    LAYOUTS,
    "worker",
    "decode",
    undefined,
    registry,
    undefined,
    callbacks,
    enumFactory,
    undefined,
    undefined,
    undefined,
    undefined,
    (handle) => foreignStub(handle, callbacks, registry),
  );
}
