export type BridgeErrorCode =
  | "contractMismatch"
  | "workerTerminated"
  | "clientClosed"
  | "storageBusy"
  | "lagged"
  | "callbackFailed"
  | "cancelled";

export interface ErrorWire {
  variant: string;
  code: string;
  category: string | number;
  retryable: boolean;
  message: string;
  details?: unknown;
}

export class BridgeError extends Error {
  constructor(
    readonly variant: string,
    readonly code: string,
    readonly category: string | number,
    readonly retryable: boolean,
    message: string,
    readonly details?: unknown,
  ) {
    super(message);
    this.name = variant;
  }
}

export function bridgeError(
  code: BridgeErrorCode,
  details?: unknown,
): BridgeError {
  return new BridgeError(code, code, "lifecycle", false, code, details);
}

export function encodeError(error: unknown): ErrorWire {
  if (error instanceof BridgeError) {
    return {
      variant: error.variant,
      code: error.code,
      category: error.category,
      retryable: error.retryable,
      message: error.message,
      details: error.details,
    };
  }
  if (error instanceof Error) {
    if ("tag" in error && typeof error.tag === "string") {
      const inner: unknown = "inner" in error ? error.inner : undefined;
      const detail: unknown = Array.isArray(inner) ? inner[0] : inner;
      if (
        detail !== null &&
        typeof detail === "object" &&
        "code" in detail &&
        typeof detail.code === "string" &&
        "category" in detail &&
        (typeof detail.category === "string" ||
          typeof detail.category === "number") &&
        "retryable" in detail &&
        typeof detail.retryable === "boolean"
      ) {
        return {
          variant: error.tag,
          code: detail.code,
          category: detail.category,
          retryable: detail.retryable,
          message: error.message,
          details: inner,
        };
      }
      return {
        variant: error.tag,
        code: "unknown",
        category: "unknown",
        retryable: false,
        message: error.message,
        details: inner,
      };
    }
    return {
      variant: error.name,
      code: "unknown",
      category: "unknown",
      retryable: false,
      message: error.message,
    };
  }
  return {
    variant: "Unknown",
    code: "unknown",
    category: "unknown",
    retryable: false,
    message: String(error),
  };
}

export function decodeError(error: ErrorWire): BridgeError {
  return new BridgeError(
    error.variant,
    error.code,
    error.category,
    error.retryable,
    error.message,
    error.details,
  );
}

export interface HandleWire {
  h: number;
  owner: number;
  epoch: number;
  type: string;
  snap?: unknown;
}

export interface CallbackWire {
  cb: number;
  type: string;
}

export type WireMessage =
  | { t: "hello"; version: number; hash: string }
  | { t: "ready"; epoch: number }
  | { t: "refused"; error: ErrorWire }
  | { t: "call"; id: number; key: string; target?: HandleWire; args: unknown[] }
  | { t: "return"; id: number; value: unknown }
  | { t: "error"; id: number; error: ErrorWire }
  | { t: "cancel"; id: number }
  | { t: "release"; handles: number[]; owners?: number[] }
  | { t: "callback"; id: number; cb: number; method: string; args: unknown[] }
  | { t: "callbackResult"; id: number; value?: unknown; error?: ErrorWire }
  | { t: "callbackDrop"; cb: number }
  | { t: "fatal"; error: ErrorWire };

export interface WireEndpoint {
  postMessage(message: WireMessage, transfer?: Transferable[]): void;
  onMessage(handler: (message: WireMessage) => void): void;
  onExit(handler: () => void): void;
  close?(): void;
  terminate?(): void;
}

export function assertCloneable(value: unknown): void {
  if (value === null || typeof value !== "object") {
    if (typeof value === "function" || typeof value === "symbol") {
      throw new TypeError("bridge payload is not cloneable");
    }
    return;
  }
  if (
    value instanceof Uint8Array ||
    value instanceof ArrayBuffer ||
    value instanceof Date
  ) {
    return;
  }
  if (Array.isArray(value)) {
    for (const item of value) assertCloneable(item);
    return;
  }
  if (value instanceof Map) {
    for (const [key, item] of value) {
      assertCloneable(key);
      assertCloneable(item);
    }
    return;
  }
  if (value instanceof Set) {
    for (const item of value) assertCloneable(item);
    return;
  }
  if (Object.getPrototypeOf(value) !== Object.prototype) {
    throw new TypeError("bridge payload has a non-plain prototype");
  }
  for (const item of Object.values(value)) assertCloneable(item);
}
