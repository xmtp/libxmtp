import { usesRustStandardFallback } from "./codec";
// The host send policy for typed codecs (Ref Public surface, Host codecs).
// Every codec step runs before the send starts, so a failed step makes no
// publish attempt.
import {
  XmtpError,
  type ContentTypeId,
  type EncodedContent,
  type SendOptions,
} from "../../public-values.gen";
import { usesRustStandardFallback } from "./codec";
import type { ContentCodec } from "./codec";
import { isCatalogueContentType } from "./host";

// A description of a codec failure. Reading the cause can itself throw, for
// example a throwing `message` getter or `toString`, so it has a fixed
// fallback.
function describe(cause: unknown): string {
  try {
    return cause instanceof Error ? String(cause.message) : String(cause);
  } catch {
    return "the failure has no readable description";
  }
}

function codecFailed(step: string, cause: unknown): XmtpError {
  return new XmtpError.CodecEncodeFailed({
    code: "CodecEncodeFailed",
    category: "callback",
    retryable: false,
    message: `content codec ${step} failed: ${describe(cause)}`,
  });
}

function isThenable(value: unknown): value is PromiseLike<unknown> {
  return (
    value !== null &&
    (typeof value === "object" || typeof value === "function") &&
    typeof Reflect.get(value, "then") === "function"
  );
}

/** A step result that its parser rejected. */
const INVALID: unique symbol = Symbol("invalid codec step result");

// A codec step is synchronous. Run it and parse its result; a throw, a
// Promise, or a result that `parse` rejects is CodecEncodeFailed. The whole
// step, including reading the result's properties, is inside the boundary,
// so a throwing getter is a codec failure too. A rejected Promise is handled
// here, so an async step cannot also end the process with an unhandled
// rejection.
function runStep<R>(
  step: string,
  run: () => unknown,
  parse: (result: unknown) => R | typeof INVALID,
): R {
  try {
    const result = run();
    if (isThenable(result)) {
      Promise.resolve(result).catch(() => undefined);
      throw new Error("the result is a Promise; codec steps are synchronous");
    }
    const parsed = parse(result);
    if (parsed === INVALID) throw new Error("the result has the wrong type");
    return parsed;
  } catch (error) {
    throw codecFailed(step, error);
  }
}

function isUint(value: unknown): value is number {
  return (
    typeof value === "number" &&
    Number.isInteger(value) &&
    value >= 0 &&
    value <= 0xffff_ffff
  );
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.length > 0;
}

// implements: CTYPE-003
// An envelope type names a non-empty authority and type ID, so a codec with an
// empty identifier fails before the send, not in the binding. Each property is
// read once, and the result is a new object, so a getter or a later change to
// the codec's object cannot change the checked value.
function parseContentTypeId(value: unknown): ContentTypeId | typeof INVALID {
  if (value === null || typeof value !== "object") return INVALID;
  const authorityId: unknown = Reflect.get(value, "authorityId");
  const typeId: unknown = Reflect.get(value, "typeId");
  const versionMajor: unknown = Reflect.get(value, "versionMajor");
  const versionMinor: unknown = Reflect.get(value, "versionMinor");
  if (
    !isNonEmptyString(authorityId) ||
    !isNonEmptyString(typeId) ||
    !isUint(versionMajor) ||
    !isUint(versionMinor)
  )
    return INVALID;
  return { authorityId, typeId, versionMajor, versionMinor };
}

function sameType(left: ContentTypeId, right: ContentTypeId): boolean {
  return (
    left.authorityId === right.authorityId &&
    left.typeId === right.typeId &&
    left.versionMajor === right.versionMajor &&
    left.versionMinor === right.versionMinor
  );
}

// Brand checks that work across realms, such as a value from an iframe or a
// Node vm context, where `instanceof` fails.
// Checked at run time: a caller outside TypeScript can pass anything.
function isObject(value: unknown): value is object {
  return (
    value !== null && (typeof value === "object" || typeof value === "function")
  );
}

// Brand-checking getters, read once. Each one throws for a value that lacks
// the internal slot, whatever its prototype or `Symbol.toStringTag` says.
function getter(owner: object, key: PropertyKey): unknown {
  return Reflect.get(Reflect.getOwnPropertyDescriptor(owner, key) ?? {}, "get");
}
const typedArrayTag = getter(
  Object.getPrototypeOf(Uint8Array.prototype) as object,
  Symbol.toStringTag,
);
const mapSize = getter(Map.prototype, "size");
function isBytes(value: unknown): value is Uint8Array {
  return (
    ArrayBuffer.isView(value) &&
    typeof typedArrayTag === "function" &&
    Reflect.apply(typedArrayTag, value, []) === "Uint8Array"
  );
}
function isMap(value: unknown): value is Map<unknown, unknown> {
  if (value === null || typeof value !== "object") return false;
  if (typeof mapSize !== "function") return false;
  try {
    Reflect.apply(mapSize, value, []);
    return true;
  } catch {
    return false;
  }
}

function parseParameters(
  value: unknown,
): ReadonlyMap<string, string> | undefined | typeof INVALID {
  if (value === undefined) return undefined;
  if (!isMap(value)) return INVALID;
  const copy = new Map<string, string>();
  for (const [key, item] of value) {
    if (typeof key !== "string" || typeof item !== "string") return INVALID;
    copy.set(key, item);
  }
  return copy;
}

// A snapshot of the codec's envelope. Later hooks run on the value, not on
// this snapshot, so a codec that keeps and changes its own envelope object
// cannot change what the policy checked. The content bytes are not copied:
// they do not decide the type or the push policy.
function parseEncodedContent(value: unknown): EncodedContent | typeof INVALID {
  if (value === null || typeof value !== "object") return INVALID;
  const type = parseContentTypeId(Reflect.get(value, "type"));
  const parameters = parseParameters(Reflect.get(value, "parameters"));
  const fallback: unknown = Reflect.get(value, "fallback");
  const content: unknown = Reflect.get(value, "content");
  if (
    type === INVALID ||
    parameters === INVALID ||
    (fallback !== undefined && typeof fallback !== "string") ||
    !isBytes(content)
  )
    return INVALID;
  return { type, parameters, fallback, content } as EncodedContent;
}

function parseFallback(value: unknown): string | undefined | typeof INVALID {
  return value === undefined || typeof value === "string" ? value : INVALID;
}

function parseBoolean(value: unknown): boolean | typeof INVALID {
  return typeof value === "boolean" ? value : INVALID;
}

/**
 * The codec's content type, read once. A throwing or invalid `type` is
 * `CodecEncodeFailed`.
 */
export function codecType<T>(codec: ContentCodec<T>): ContentTypeId {
  return runStep("type", () => codec.type, parseContentTypeId);
}

/**
 * The envelope of `value` for a send. An envelope that already has a fallback
 * keeps it, and `fallback` is not called; otherwise `fallback(value)` supplies
 * it. A failed or invalid step is `CodecEncodeFailed`.
 */
export function encodeForSend<T>(
  codec: ContentCodec<T>,
  value: T,
  type: ContentTypeId = codecType(codec),
): EncodedContent {
  const encoded = runStep(
    "encode",
    () => codec.encode(value),
    parseEncodedContent,
  );
  // implements: CTYPE-007
  // The envelope type is the codec's type. A codec cannot send another type,
  // so its push hook cannot steer catalogue dispatch.
  if (!sameType(encoded.type, type))
    throw codecFailed(
      "encode",
      "the envelope type differs from the codec type",
    );
  // Decide first, then read the hook only when it will be called: an
  // envelope that has a fallback skips the hook, so a throwing or invalid
  // `fallback` member does not fail that send.
  if (encoded.fallback !== undefined) return encoded;
  const hook = runStep(
    "fallback",
    () => {
      const fallback = codec.fallback;
      return usesRustStandardFallback(codec, fallback) ? undefined : fallback;
    },
    parseHook<T>,
  );
  if (hook === undefined) return encoded;
  // Call each hook on its codec, so a class codec can use `this`.
  const fallback = runStep(
    "fallback",
    () => hook.call(codec, value),
    parseFallback,
  );
  return fallback === undefined ? encoded : { ...encoded, fallback };
}

function parseHook<T>(
  value: unknown,
): ((value: T) => unknown) | undefined | typeof INVALID {
  if (value === undefined) return undefined;
  return typeof value === "function"
    ? (value as (value: T) => unknown)
    : INVALID;
}

/**
 * The send options for `value`. An explicit `shouldPush`, including `false`,
 * wins. A catalogue type keeps its catalogue default. Otherwise the codec's
 * `shouldPush` hook decides, when it has one. A failed or invalid hook is
 * `CodecEncodeFailed`.
 */
export function optionsForSend<T>(
  codec: ContentCodec<T>,
  value: T,
  options: SendOptions | undefined,
  isCatalogue: (type: ContentTypeId) => boolean,
  type: ContentTypeId = codecType(codec),
): SendOptions | undefined {
  // Decide first, then read the hook only when it will be called: an
  // explicit option or a catalogue type skips it.
  if (options?.shouldPush !== undefined || isCatalogue(type)) return options;
  const hook = runStep("shouldPush", () => codec.shouldPush, parseHook<T>);
  if (hook === undefined) return options;
  const shouldPush = runStep(
    "shouldPush",
    () => hook.call(codec, value),
    parseBoolean,
  );
  return { ...options, shouldPush };
}

// A codec is an object or function with an `encode` member. The check does not call a
// getter, and a check that throws (a Proxy trap) is a codec failure. A value
// that is not an object, such as `null` or a string passed by mistake, is not
// a codec, so the binding gives it the input error it had before.
export function isCodec<T>(
  content: EncodedContent | ContentCodec<T> | string,
): content is ContentCodec<T> {
  if (!isObject(content)) return false;
  try {
    return "encode" in content;
  } catch (error) {
    throw codecFailed("encode", error);
  }
}

/**
 * The envelope and options of a Group or Dm send (Decision 23). A typed codec
 * runs every step before the send: encode, fallback, then push, where the
 * catalogue predicate (Decision 24) keeps a catalogue type's default. An
 * envelope send keeps its policy.
 */
export function contentForSend<T>(
  content: EncodedContent | ContentCodec<T>,
  valueOrOptions: T | SendOptions | undefined,
  options: SendOptions | undefined,
): [EncodedContent, SendOptions | undefined] {
  if (!isCodec(content))
    return [content, valueOrOptions as SendOptions | undefined];
  const value = valueOrOptions as T;
  // Read the codec's type once, for the envelope check and the push choice.
  const type = codecType(content);
  const encoded = encodeForSend(content, value, type);
  return [
    encoded,
    optionsForSend(content, value, options, isCatalogueContentType, type),
  ];
}
