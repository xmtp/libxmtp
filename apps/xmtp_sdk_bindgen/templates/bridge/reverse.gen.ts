import { ValueCodec, enumFactory } from "./runtime/bridge/codec.js";
import type { CallbackTarget } from "./runtime/bridge/main/callbacks.js";
import type { MainSession } from "./runtime/bridge/main/session.js";
import type { CallbackWire } from "./runtime/bridge/wire.js";
import { FOREIGN_METHODS, LAYOUTS, type ForeignMethod } from "./wire.gen.js";
import * as B from "./xmtp_sdk.js";

const traits: Partial<Record<string, Record<string, ForeignMethod>>> =
  FOREIGN_METHODS;

export function registerForeign(
  type: string,
  target: object,
  session: MainSession,
): CallbackWire {
  const methods = traits[type];
  if (!methods) throw new TypeError(`unknown foreign trait ${type}`);
  const wrapper: CallbackTarget = {};
  for (const [name, method] of Object.entries(methods)) {
    wrapper[name] = async (...wireArgs: unknown[]): Promise<unknown> => {
      const decoder = new ValueCodec(
        LAYOUTS,
        "main",
        "decode",
        session,
        undefined,
        undefined,
        undefined,
        enumFactory(B),
      );
      const args = method.inputs.map((shape, index) =>
        decoder.convert(shape, wireArgs[index]),
      );
      const callback: unknown = Reflect.get(target, name);
      if (typeof callback !== "function") {
        throw new TypeError(`missing ${type}.${name} callback`);
      }
      const result: unknown = await Reflect.apply(callback, target, args);
      return new ValueCodec(
        LAYOUTS,
        "main",
        "encode",
        session,
        undefined,
        session.callbacks,
      ).convert(method.output, result);
    };
  }
  return session.callbacks.register(type, wrapper);
}
