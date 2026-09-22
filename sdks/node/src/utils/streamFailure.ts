const streamFailureMarker = "\n[XMTP_STREAM_FAILURE_V1]";
const maxCursor = (1n << 64n) - 1n;

export type StreamFailureCause = {
  kind:
    | "target_pending"
    | "receipt_pending"
    | "processing_pending"
    | "blocked"
    | "storage"
    | "receiver"
    | "invalid_topic";
  code: string | null;
  message: string;
  retryable: boolean;
};

export type UnfinishedStreamTopic = {
  topic: Uint8Array;
  /** Null means that target capture did not complete. */
  target: bigint | null;
  received: bigint;
  processed: bigint;
  unresolvedWelcomes: bigint[];
  inactive: boolean;
  cause: StreamFailureCause | null;
};

export type StreamBarrierFailure = {
  reason: "blocked" | "deadline" | "cancelled";
  unfinished: UnfinishedStreamTopic[];
};

export type StreamFailureDetails = {
  kind: "barrier" | "published_but_unconfirmed" | "catch_up";
  code: string;
  message: string;
  retryable: boolean;
  intentId: number | null;
  publishedIntentIds: number[];
  summary: {
    messages: bigint;
    conversations: bigint;
    failed: bigint;
    completed: boolean;
  } | null;
  barriers: StreamBarrierFailure[];
};

/**
 * Reads structured progress from a stream, sync, or publication error.
 * All sequence values are exact bigints. Invalid or absent details return undefined.
 * A published-but-unconfirmed error does not mean that publication failed.
 */
export const getStreamFailureDetails = (
  error: unknown,
): StreamFailureDetails | undefined => {
  try {
    const message =
      typeof error === "string" ? error : readObject(error).message;
    if (typeof message !== "string") return undefined;
    const marker = message.lastIndexOf(streamFailureMarker);
    if (marker === -1) return undefined;
    const details = readObject(
      JSON.parse(message.slice(marker + streamFailureMarker.length)),
    );
    return {
      kind: readChoice(details.kind, [
        "barrier",
        "published_but_unconfirmed",
        "catch_up",
      ]),
      code: readString(details.code),
      message: readString(details.message),
      retryable: readBoolean(details.retryable),
      intentId: readNullable(details.intentId, readIntentId),
      publishedIntentIds: readArray(details.publishedIntentIds, readIntentId),
      summary: readNullable(details.summary, (value) => {
        const summary = readObject(value);
        return {
          messages: readCursor(summary.messages),
          conversations: readCursor(summary.conversations),
          failed: readCursor(summary.failed),
          completed: readBoolean(summary.completed),
        };
      }),
      barriers: readArray(details.barriers, readBarrier),
    };
  } catch {
    return undefined;
  }
};

const readBarrier = (value: unknown): StreamBarrierFailure => {
  const barrier = readObject(value);
  return {
    reason: readChoice(barrier.reason, ["blocked", "deadline", "cancelled"]),
    unfinished: readArray(barrier.unfinished, (value) => {
      const topic = readObject(value);
      return {
        topic: readTopic(topic.topic),
        target: readNullable(topic.target, readCursor),
        received: readCursor(topic.received),
        processed: readCursor(topic.processed),
        unresolvedWelcomes: readArray(topic.unresolvedWelcomes, readCursor),
        inactive: readBoolean(topic.inactive),
        cause: readNullable(topic.cause, (value) => {
          const cause = readObject(value);
          return {
            kind: readChoice(cause.kind, [
              "target_pending",
              "receipt_pending",
              "processing_pending",
              "blocked",
              "storage",
              "receiver",
              "invalid_topic",
            ]),
            code: readNullable(cause.code, readString),
            message: readString(cause.message),
            retryable: readBoolean(cause.retryable),
          };
        }),
      };
    }),
  };
};

const readObject = (value: unknown): Record<string, unknown> => {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error("Invalid stream failure object");
  }
  return value as Record<string, unknown>;
};

const readString = (value: unknown): string => {
  if (typeof value !== "string") throw new Error("Invalid stream failure text");
  return value;
};

const readBoolean = (value: unknown): boolean => {
  if (typeof value !== "boolean")
    throw new Error("Invalid stream failure flag");
  return value;
};

const readChoice = <T extends string>(
  value: unknown,
  choices: readonly T[],
): T => {
  const choice = choices.find((choice) => choice === value);
  if (choice === undefined) throw new Error("Invalid stream failure kind");
  return choice;
};

const readNullable = <T>(
  value: unknown,
  read: (value: unknown) => T,
): T | null => (value === null ? null : read(value));

const readArray = <T>(value: unknown, read: (value: unknown) => T): T[] => {
  if (!Array.isArray(value)) throw new Error("Invalid stream failure list");
  return value.map(read);
};

const readCursor = (value: unknown): bigint => {
  const cursor = readString(value);
  if (!/^(0|[1-9][0-9]*)$/.test(cursor)) {
    throw new Error("Invalid stream failure sequence");
  }
  const number = BigInt(cursor);
  if (number > maxCursor)
    throw new Error("Stream failure sequence is too large");
  return number;
};

const readIntentId = (value: unknown): number => {
  if (
    typeof value !== "number" ||
    !Number.isInteger(value) ||
    value < -2147483648 ||
    value > 2147483647
  ) {
    throw new Error("Invalid stream failure intent ID");
  }
  return value;
};

const readTopic = (value: unknown): Uint8Array => {
  const hex = readString(value);
  if (!/^(?:[0-9a-fA-F]{2})*$/.test(hex)) {
    throw new Error("Invalid stream failure topic");
  }
  const bytes = new Uint8Array(hex.length / 2);
  for (let index = 0; index < bytes.length; index++) {
    bytes[index] = Number.parseInt(hex.slice(index * 2, index * 2 + 2), 16);
  }
  return bytes;
};
