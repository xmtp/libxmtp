import { registerForeign } from "./reverse.gen.js";
import { ValueCodec, type EnumFactory } from "./runtime/bridge/codec.js";
import type { MainSession } from "./runtime/bridge/main/session.js";
import type { HandleWire } from "./runtime/bridge/wire.js";
import { LAYOUTS } from "./wire.gen.js";

export function mainEncoder(session: MainSession): ValueCodec {
  return new ValueCodec(
    LAYOUTS,
    "main",
    "encode",
    session,
    undefined,
    session.callbacks,
    undefined,
    undefined,
    undefined,
    undefined,
    undefined,
    (name, value) => registerForeign(name, value, session),
  );
}

export function mainDecoder(
  session: MainSession,
  enumFactory: EnumFactory,
  objectFactory: (handle: HandleWire) => unknown,
): ValueCodec {
  return new ValueCodec(
    LAYOUTS,
    "main",
    "decode",
    session,
    undefined,
    undefined,
    undefined,
    enumFactory,
    undefined,
    objectFactory,
  );
}
