import { ValueCodec, enumFactory } from "./runtime/bridge/codec.js";
import type { CallbackWire } from "./runtime/bridge/wire.js";
import {
  LogWindow,
  type WorkerCallbacks,
} from "./runtime/bridge/worker/callback-stub.js";
import type { WorkerRegistry } from "./runtime/bridge/worker/registry.js";
import { FOREIGN_METHODS, LAYOUTS, type ForeignMethod } from "./wire.gen.js";
import * as B from "./xmtp_sdk.js";

const traits: Partial<Record<string, Partial<Record<string, ForeignMethod>>>> =
  FOREIGN_METHODS;
const collected = new FinalizationRegistry<{
  callbacks: WorkerCallbacks;
  cb: number;
}>(({ callbacks, cb }) => callbacks.drop(cb));

export function foreignStub(
  handle: CallbackWire,
  callbacks: WorkerCallbacks,
  registry: WorkerRegistry,
): object {
  const methods = traits[handle.type];
  if (!methods) throw new TypeError(`unknown foreign trait ${handle.type}`);
  if (handle.type === "LogSink") {
    const window = new LogWindow(callbacks, handle.cb);
    const encoder = new ValueCodec(
      LAYOUTS,
      "worker",
      "encode",
      undefined,
      registry,
    );
    const sink = {
      log(record: unknown): void {
        const wire = encoder.convert(
          { kind: "record", name: "LogRecord" },
          record,
        );
        if (window.log(wire) === "busy") throw B.LogSinkError.Busy.new();
      },
    };
    collected.register(sink, { callbacks, cb: handle.cb });
    return sink;
  }
  const stub = new Proxy(
    {},
    {
      get(_target, property) {
        if (typeof property !== "string" || property === "then")
          return undefined;
        const method = methods[property];
        if (!method) return undefined;
        return async (...args: unknown[]): Promise<unknown> => {
          const encoder = new ValueCodec(
            LAYOUTS,
            "worker",
            "encode",
            undefined,
            registry,
          );
          const wireArgs = method.inputs.map((shape, index) =>
            encoder.convert(shape, args[index]),
          );
          const raw = await callbacks.invoke(handle.cb, property, wireArgs);
          return new ValueCodec(
            LAYOUTS,
            "worker",
            "decode",
            undefined,
            registry,
            undefined,
            callbacks,
            enumFactory(B),
          ).convert(method.output, raw);
        };
      },
    },
  );
  collected.register(stub, { callbacks, cb: handle.cb });
  return stub;
}
