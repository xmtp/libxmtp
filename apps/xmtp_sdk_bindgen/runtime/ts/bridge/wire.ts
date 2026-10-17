import { UniffiInternalError } from "@ubjs/core";

import { ErrorCategory } from "../../xmtp_sdk.js";

function bridgeCodes<const T extends readonly string[]>(...codes: T): T {
  return codes;
}

export const BRIDGE_ERROR_CODES = bridgeCodes(
  "contractMismatch",
  "workerTerminated",
  "clientClosed",
  "storageBusy",
  "lagged",
  "callbackFailed",
  "cancelled",
);

export type BridgeErrorCode = (typeof BRIDGE_ERROR_CODES)[number];

/**
 * Reader reads whose admitted value is abandoned when the owner client ends.
 * Client end waits for their database work, not for their reply to reach the
 * app. The value stays unacknowledged, so a later reader or replay sees it.
 */
const ABANDONED_AT_END = new Set([
  "MessageReader.next",
  "ConversationReader.next",
  "EventReader.next",
]);

export function abandonedAtEnd(key: string): boolean {
  return ABANDONED_AT_END.has(key);
}

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
  const publicCode = code[0].toUpperCase() + code.slice(1);
  if (code === "clientClosed") {
    // Keep these fields in sync with XmtpError::closed in the SDK façade.
    const category = ErrorCategory.Lifecycle;
    return new BridgeError(
      "ClientClosed",
      "ClientClosed",
      category,
      false,
      code,
      [
        {
          code: "ClientClosed",
          category,
          retryable: false,
          message: "client is closed",
        },
      ],
    );
  }
  if (code === "storageBusy") {
    const category = ErrorCategory.Storage;
    return new BridgeError("StorageBusy", publicCode, category, true, code, [
      { code: publicCode, category, retryable: true, message: code },
    ]);
  }
  if (code === "lagged") {
    const category = ErrorCategory.Stream;
    return new BridgeError("Lagged", "Lagged", category, true, code, [
      { code: "Lagged", category, retryable: true, message: code },
    ]);
  }
  return new BridgeError(
    publicCode,
    publicCode,
    ErrorCategory.Lifecycle,
    false,
    code,
    details,
  );
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
        code: "Unknown",
        category: ErrorCategory.Unknown,
        retryable: false,
        message: error.message,
        details: inner,
      };
    }
    // An aborted binding call (UniFFI's AbortError) is a cancellation, the
    // same as an abort that the main thread sees first.
    if (error instanceof UniffiInternalError.AbortError)
      return {
        variant: "Cancelled",
        code: "Cancelled",
        category: ErrorCategory.Lifecycle,
        retryable: false,
        message: error.message,
      };
    return {
      variant: error.name,
      code: "Unknown",
      category: ErrorCategory.Unknown,
      retryable: false,
      message: error.message,
    };
  }
  return {
    variant: "Unknown",
    code: "Unknown",
    category: ErrorCategory.Unknown,
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
  | { t: "hello"; version: number; hash: string; lifetimeLock?: string }
  | { t: "ready"; epoch: number }
  | { t: "idle"; revision: number }
  | { t: "refused"; error: ErrorWire }
  | {
      t: "call";
      id: number;
      key: string;
      target?: HandleWire;
      args: unknown[];
      revision?: number;
    }
  | { t: "return"; id: number; value: unknown }
  | { t: "error"; id: number; error: ErrorWire; fatal?: boolean }
  | { t: "cancel"; id: number }
  | { t: "release"; handles: number[]; owners?: number[]; revision?: number }
  | { t: "callback"; id: number; cb: number; method: string; args: unknown[] }
  | { t: "logHandoff"; id: number }
  | { t: "callbackResult"; id: number; value?: unknown; error?: ErrorWire }
  | { t: "callbackDrop"; cb: number }
  | { t: "fatal"; error: ErrorWire };

export interface WireEndpoint {
  postMessage(message: WireMessage, transfer?: Transferable[]): void;
  onMessage(handler: (message: WireMessage) => void): void;
  onExit(handler: () => void): void;
  close?(): void;
  terminate?(): void | Promise<void>;
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
