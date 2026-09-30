// The host send policy for typed codecs (Ref Public surface, Host codecs).
// Every codec step runs before the send starts, so a failed step makes no
// publish attempt.
import {
  XmtpError,
  type ContentTypeId,
  type EncodedContent,
  type SendOptions,
} from "../../public-values.gen";
import type { ContentCodec } from "./codec";
import { isCatalogueContentType } from "./host";

function codecFailed(step: string, cause: unknown): XmtpError {
  const reason = cause instanceof Error ? cause.message : String(cause);
  return new XmtpError.CodecEncodeFailed({
    code: "CodecEncodeFailed",
    category: "callback",
    retryable: false,
    message: `content codec ${step} failed: ${reason}`,
  });
}

function isThenable(value: unknown): value is PromiseLike<unknown> {
  return (
    value !== null &&
    (typeof value === "object" || typeof value === "function") &&
    typeof Reflect.get(value, "then") === "function"
  );
}

// A codec step is synchronous. Run it; a throw, a Promise, or a result that
// `valid` rejects is CodecEncodeFailed. A rejected Promise is handled here, so
// an async step cannot also end the process with an unhandled rejection.
function runStep<R>(
  step: string,
  run: () => unknown,
  valid: (result: unknown) => result is R,
): R {
  let result: unknown;
  try {
    result = run();
  } catch (error) {
    throw codecFailed(step, error);
  }
  if (isThenable(result)) {
    Promise.resolve(result).catch(() => undefined);
    throw codecFailed(
      step,
      "the result is a Promise; codec steps are synchronous",
    );
  }
  if (!valid(result)) throw codecFailed(step, "the result has the wrong type");
  return result;
}

function isUint(value: unknown): value is number {
  return (
    typeof value === "number" &&
    Number.isInteger(value) &&
    value >= 0 &&
    value <= 0xffff_ffff
  );
}

function isContentTypeId(value: unknown): value is ContentTypeId {
  return (
    value !== null &&
    typeof value === "object" &&
    typeof Reflect.get(value, "authorityId") === "string" &&
    typeof Reflect.get(value, "typeId") === "string" &&
    isUint(Reflect.get(value, "versionMajor")) &&
    isUint(Reflect.get(value, "versionMinor"))
  );
}

function sameType(left: ContentTypeId, right: ContentTypeId): boolean {
  return (
    left.authorityId === right.authorityId &&
    left.typeId === right.typeId &&
    left.versionMajor === right.versionMajor &&
    left.versionMinor === right.versionMinor
  );
}

function isParameters(value: unknown): boolean {
  if (value === undefined) return true;
  if (!(value instanceof Map)) return false;
  for (const [key, item] of value)
    if (typeof key !== "string" || typeof item !== "string") return false;
  return true;
}

function isEncodedContent(value: unknown): value is EncodedContent {
  if (value === null || typeof value !== "object") return false;
  const fallback: unknown = Reflect.get(value, "fallback");
  return (
    isContentTypeId(Reflect.get(value, "type")) &&
    isParameters(Reflect.get(value, "parameters")) &&
    (fallback === undefined || typeof fallback === "string") &&
    Reflect.get(value, "content") instanceof Uint8Array
  );
}

function isFallback(value: unknown): value is string | undefined {
  return value === undefined || typeof value === "string";
}

function isBoolean(value: unknown): value is boolean {
  return typeof value === "boolean";
}

/**
 * The envelope of `value` for a send. An envelope that already has a fallback
 * keeps it, and `fallback` is not called; otherwise `fallback(value)` supplies
 * it. A failed or invalid step is `CodecEncodeFailed`.
 */
export function encodeForSend<T>(
  codec: ContentCodec<T>,
  value: T,
): EncodedContent {
  const encoded = runStep(
    "encode",
    () => codec.encode(value),
    isEncodedContent,
  );
  // implements: CTYPE-007
  // The envelope type is the codec's type. A codec cannot send another type,
  // so its push hook cannot steer catalogue dispatch.
  if (!sameType(encoded.type, codec.type))
    throw codecFailed(
      "encode",
      "the envelope type differs from the codec type",
    );
  const hook = codec.fallback;
  if (encoded.fallback !== undefined || hook === undefined) return encoded;
  // Call each hook on its codec, so a class codec can use `this`.
  const fallback = runStep(
    "fallback",
    () => hook.call(codec, value),
    isFallback,
  );
  return fallback === undefined ? encoded : { ...encoded, fallback };
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
): SendOptions | undefined {
  const hook = codec.shouldPush;
  if (options?.shouldPush !== undefined || hook === undefined) return options;
  if (isCatalogue(codec.type)) return options;
  const shouldPush = runStep(
    "shouldPush",
    () => hook.call(codec, value),
    isBoolean,
  );
  return { ...options, shouldPush };
}

function isCodec<T>(
  content: EncodedContent | ContentCodec<T>,
): content is ContentCodec<T> {
  return typeof Reflect.get(content, "encode") === "function";
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
  const encoded = encodeForSend(content, value);
  return [
    encoded,
    optionsForSend(content, value, options, isCatalogueContentType),
  ];
}
