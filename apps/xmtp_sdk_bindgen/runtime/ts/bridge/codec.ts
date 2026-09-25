import type { MainCallbacks } from "./main/callbacks.js";
import { RemoteObject } from "./main/remote-object.js";
import type { MainSession } from "./main/session.js";
import {
  decodeError,
  encodeError,
  type CallbackWire,
  type ErrorWire,
  type HandleWire,
} from "./wire.js";
import type { WorkerCallbacks } from "./worker/callback-stub.js";
import type { WorkerRegistry } from "./worker/registry.js";

export type Shape =
  | { kind: "value"; type?: string }
  | { kind: "custom"; name: string; inner: Shape }
  | { kind: "object"; name: string }
  | { kind: "foreign"; name: string }
  | { kind: "callback"; name: string }
  | { kind: "record"; name: string }
  | { kind: "enum"; name: string }
  | { kind: "optional" | "sequence" | "set"; inner: Shape }
  | { kind: "map"; key: Shape; value: Shape };

export interface RecordLayout {
  fields: Record<string, Shape>;
}

export interface EnumLayout {
  variants: Partial<Record<string, Record<string, Shape> | Shape[]>>;
  error: boolean;
  flat: boolean;
}

export interface Layouts {
  records: Partial<Record<string, RecordLayout>>;
  enums: Partial<Record<string, EnumLayout>>;
}

export interface EnumFactory {
  (name: string, tag: string, fields: unknown): unknown;
  custom?(name: string, value: unknown): unknown;
}

export function enumFactory(binding: object): EnumFactory {
  const tagged: EnumFactory = (name, tag, fields) => {
    const enumeration: unknown = Reflect.get(binding, name);
    if (enumeration === null || typeof enumeration !== "object") {
      throw new TypeError(`unknown binding enum ${name}`);
    }
    const constructor: unknown = Reflect.get(enumeration, tag);
    if (typeof constructor !== "function")
      throw new TypeError(`unknown ${name} tag ${tag}`);
    const constructed: unknown = Reflect.construct(
      constructor,
      Array.isArray(fields) ? fields : [fields],
    );
    return constructed;
  };
  tagged.custom = (name, value) => {
    const host: unknown = Reflect.get(binding, name);
    if (typeof host !== "function")
      throw new TypeError(`unknown binding custom type ${name}`);
    const fromRust: unknown = Reflect.get(host, "fromRust");
    return typeof fromRust === "function"
      ? Reflect.apply(fromRust, host, [value])
      : Reflect.construct(host, [value]);
  };
  return tagged;
}

function plain(value: unknown): Record<string, unknown> {
  if (value === null || typeof value !== "object") {
    throw new TypeError("expected bridge record");
  }
  if (Array.isArray(value)) throw new TypeError("expected bridge record");
  return Object.fromEntries(Object.entries(value));
}

function handle(value: unknown): HandleWire {
  const fields = plain(value);
  if (
    typeof fields.h !== "number" ||
    typeof fields.owner !== "number" ||
    typeof fields.epoch !== "number" ||
    typeof fields.type !== "string"
  ) {
    throw new TypeError("invalid bridge handle");
  }
  return {
    h: fields.h,
    owner: fields.owner,
    epoch: fields.epoch,
    type: fields.type,
    snap: fields.snap,
  };
}

function errorWire(value: unknown): ErrorWire {
  const fields = plain(value);
  if (
    typeof fields.variant !== "string" ||
    typeof fields.code !== "string" ||
    (typeof fields.category !== "string" &&
      typeof fields.category !== "number") ||
    typeof fields.retryable !== "boolean" ||
    typeof fields.message !== "string"
  ) {
    throw new TypeError("invalid error wire value");
  }
  return {
    variant: fields.variant,
    code: fields.code,
    category: fields.category,
    retryable: fields.retryable,
    message: fields.message,
    details: fields.details,
  };
}

export class ValueCodec {
  constructor(
    private readonly layouts: Layouts,
    private readonly side: "main" | "worker",
    private readonly direction: "encode" | "decode",
    private readonly session?: MainSession,
    private readonly registry?: WorkerRegistry,
    private readonly mainCallbacks?: MainCallbacks,
    private readonly workerCallbacks?: WorkerCallbacks,
    private readonly enumFactory?: EnumFactory,
    private readonly owner?: number,
    private readonly objectFactory?: (handle: HandleWire) => unknown,
    private readonly snapshotFactory?: (
      name: string,
      value: object,
      owner: number,
    ) => unknown,
    private readonly mainForeignFactory?: (
      name: string,
      value: object,
    ) => CallbackWire,
    private readonly workerForeignFactory?: (handle: CallbackWire) => unknown,
  ) {}

  convert(shape: Shape, value: unknown): unknown {
    switch (shape.kind) {
      case "value":
        if (shape.type === "Bytes" && this.direction === "encode") {
          if (value instanceof Uint8Array) return value.slice();
          if (value instanceof ArrayBuffer) return value.slice(0);
        }
        return value;
      case "custom": {
        if (this.direction === "encode") {
          if (value === null || typeof value !== "object")
            throw new TypeError(`expected ${shape.name}`);
          const stringify: unknown = Reflect.get(value, "toString");
          if (typeof stringify !== "function")
            throw new TypeError(`invalid ${shape.name}`);
          const inner: unknown =
            shape.name === "Message"
              ? Reflect.get(value, "data")
              : shape.name === "Timestamp"
                ? Reflect.get(value, "ns")
                : Reflect.apply(stringify, value, []);
          return this.convert(shape.inner, inner);
        }
        const inner = this.convert(shape.inner, value);
        return this.enumFactory?.custom?.(shape.name, inner) ?? inner;
      }
      case "object":
        return this.object(shape.name, value);
      case "foreign":
        return this.foreign(shape.name, value);
      case "callback":
        return this.callback(shape.name, value);
      case "record": {
        const fields = plain(value);
        const layout = this.layouts.records[shape.name];
        if (!layout) throw new TypeError(`unknown record ${shape.name}`);
        const output: Record<string, unknown> = {};
        for (const [name, field] of Object.entries(layout.fields)) {
          output[name] = this.convert(field, fields[name]);
        }
        return output;
      }
      case "enum": {
        const layout = this.layouts.enums[shape.name];
        if (!layout) throw new TypeError(`unknown enum ${shape.name}`);
        if (layout.flat) {
          if (
            typeof value !== "number" ||
            !Number.isInteger(value) ||
            value < 0 ||
            value >= Object.keys(layout.variants).length
          ) {
            throw new TypeError(`invalid ${shape.name} value`);
          }
          return value;
        }
        if (layout.error) {
          if (this.direction === "encode") {
            const error = encodeError(value);
            const variant = layout.variants[error.variant];
            if (!variant || !Array.isArray(variant))
              throw new TypeError(`unknown ${shape.name} error`);
            const details = Array.isArray(error.details)
              ? error.details
              : error.details === undefined
                ? []
                : [error.details];
            return {
              variant: error.variant,
              code: error.code,
              category: error.category,
              retryable: error.retryable,
              message: error.message,
              details: variant.map((field, index) =>
                this.convert(field, details[index]),
              ),
            };
          }
          const error = errorWire(value);
          const variant = layout.variants[error.variant];
          if (!variant || !Array.isArray(variant))
            throw new TypeError(`unknown ${shape.name} error`);
          const details = error.details;
          if (!Array.isArray(details))
            throw new TypeError(`invalid ${shape.name} error details`);
          if (!variant[0])
            return (
              this.enumFactory?.(shape.name, error.variant, []) ??
              decodeError(error)
            );
          const detail = this.convert(variant[0], details[0]);
          return (
            this.enumFactory?.(shape.name, error.variant, [detail]) ??
            decodeError(error)
          );
        }
        const fields = plain(value);
        const tag = fields.tag;
        if (typeof tag !== "string")
          throw new TypeError(`enum ${shape.name} has no tag`);
        const variant = layout.variants[tag];
        if (!variant) throw new TypeError(`unknown ${shape.name} tag ${tag}`);
        let output: unknown;
        if (Array.isArray(variant)) {
          const inner = fields.inner;
          if (!Array.isArray(inner))
            throw new TypeError(`invalid ${shape.name} tuple`);
          output = variant.map((field, index) =>
            this.convert(field, inner[index]),
          );
        } else {
          const inner =
            Object.keys(variant).length > 0 ? plain(fields.inner) : {};
          const record: Record<string, unknown> = {};
          for (const [name, field] of Object.entries(variant)) {
            record[name] = this.convert(field, inner[name]);
          }
          output = record;
        }
        if (this.direction === "decode" && this.enumFactory) {
          return this.enumFactory(shape.name, tag, output);
        }
        return Object.keys(variant).length > 0
          ? { tag, inner: output }
          : { tag };
      }
      case "optional":
        return value === null || value === undefined
          ? value
          : this.convert(shape.inner, value);
      case "sequence":
        if (!Array.isArray(value)) throw new TypeError("expected sequence");
        return value.map((item) => this.convert(shape.inner, item));
      case "set":
        if (!(value instanceof Set)) throw new TypeError("expected set");
        return new Set(
          Array.from(value, (item) => this.convert(shape.inner, item)),
        );
      case "map":
        if (!(value instanceof Map)) throw new TypeError("expected map");
        return new Map(
          Array.from(value, ([key, item]) => [
            this.convert(shape.key, key),
            this.convert(shape.value, item),
          ]),
        );
    }
  }

  private object(name: string, value: unknown): unknown {
    if (this.side === "main" && this.direction === "encode") {
      if (!(value instanceof RemoteObject))
        throw new TypeError(`expected ${name} proxy`);
      value.checkLive(name, this.session);
      return value.handle;
    }
    if (this.side === "worker" && this.direction === "decode") {
      if (!this.registry) throw new Error("worker registry missing");
      return this.registry.get(handle(value));
    }
    if (this.side === "worker" && this.direction === "encode") {
      if (!this.registry || value === null || typeof value !== "object") {
        throw new TypeError(`expected ${name} object`);
      }
      return this.registry.add(value, name, this.owner, (owner) =>
        this.snapshotFactory?.(name, value, owner),
      );
    }
    if (!this.session) throw new Error("main session missing");
    const parsed = handle(value);
    return (
      this.objectFactory?.(parsed) ?? new RemoteObject(this.session, parsed)
    );
  }

  private callback(name: string, value: unknown): unknown {
    if (this.side === "main" && this.direction === "encode") {
      if (!this.mainCallbacks || value === null || typeof value !== "object") {
        throw new TypeError(`expected ${name} callback`);
      }
      if (this.mainForeignFactory) return this.mainForeignFactory(name, value);
      return this.mainCallbacks.register(name, value);
    }
    if (this.side === "worker" && this.direction === "decode") {
      const fields = plain(value);
      if (typeof fields.cb !== "number" || !this.workerCallbacks) {
        throw new TypeError("invalid callback handle");
      }
      const cb = fields.cb;
      if (this.workerForeignFactory) {
        return this.workerForeignFactory({ cb, type: name });
      }
      const callbacks = this.workerCallbacks;
      return new Proxy(
        {},
        {
          get(_target, property) {
            if (typeof property !== "string" || property === "then")
              return undefined;
            return (...args: unknown[]) => callbacks.invoke(cb, property, args);
          },
        },
      );
    }
    if (this.side === "worker" && this.direction === "encode") {
      throw new TypeError("worker callback exports need a generated stub");
    }
    const fields = plain(value);
    if (typeof fields.cb !== "number")
      throw new TypeError("invalid callback handle");
    return { cb: fields.cb, type: name } satisfies CallbackWire;
  }

  private foreign(name: string, value: unknown): unknown {
    if (this.side === "main" && this.direction === "encode") {
      if (value instanceof RemoteObject) {
        value.checkLive(name, this.session);
        return value.handle;
      }
      if (!this.mainCallbacks || value === null || typeof value !== "object") {
        throw new TypeError(`expected ${name} callback`);
      }
      if (this.mainForeignFactory) return this.mainForeignFactory(name, value);
      return this.mainCallbacks.register(name, value);
    }
    if (this.side === "worker" && this.direction === "decode") {
      const fields = plain(value);
      if (typeof fields.h === "number") return this.object(name, value);
      if (typeof fields.cb !== "number" || !this.workerCallbacks) {
        throw new TypeError("invalid foreign handle");
      }
      const cb = fields.cb;
      if (this.workerForeignFactory) {
        return this.workerForeignFactory({ cb, type: name });
      }
      const callbacks = this.workerCallbacks;
      return new Proxy(
        {},
        {
          get(_target, property) {
            if (typeof property !== "string" || property === "then")
              return undefined;
            return (...args: unknown[]) => callbacks.invoke(cb, property, args);
          },
        },
      );
    }
    return this.object(name, value);
  }
}
