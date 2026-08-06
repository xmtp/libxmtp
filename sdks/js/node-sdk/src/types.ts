import type { ContentCodec } from "@xmtp/content-type-primitives";
import {
  type Actions,
  type Attachment,
  type Backend,
  type ContentTypeId,
  type DeletedMessage,
  type GroupUpdated,
  type Intent,
  type LeaveRequest,
  type LogLevel,
  type MultiRemoteAttachment,
  type Reaction,
  type ReadReceipt,
  type RemoteAttachment,
  type TransactionReference,
  type VisibilityConfirmationOptions,
  type WalletSendCalls,
  type WorkerConfigOptions,
} from "@xmtp/node-bindings";
import type { DecodedMessage } from "@/DecodedMessage";
import type { HexString } from "./utils/validation";

/**
 * XMTP environment
 */
export type XmtpEnv =
  | "local"
  | "dev"
  | "production"
  | "testnet-staging"
  | "testnet-dev"
  | "testnet"
  | "mainnet";

/**
 * Network options
 */
export type NetworkOptions = {
  /**
   * Specify which XMTP environment to connect to. (default: `dev`)
   *
   * @see https://docs.xmtp.org/chat-apps/core-messaging/create-a-client#xmtp-network-environments
   */
  env?: XmtpEnv;
  /**
   * apiUrl can be used to override the `env` flag and connect to a
   * specific endpoint
   */
  apiUrl?: string;
  /**
   * The host of the XMTP Gateway for your application
   *
   * Only valid for `dev` and `production` environments
   *
   * @see https://docs.xmtp.org/fund-agents-apps/run-gateway
   */
  gatewayHost?: string;
  /**
   * Custom app version
   */
  appVersion?: string;
};

/**
 * Device sync options
 */
export type DeviceSyncOptions = {
  /**
   * historySyncUrl can be used to override the `env` flag and connect to a
   * specific endpoint for syncing history
   *
   * @see https://docs.xmtp.org/chat-apps/list-stream-sync/history-sync
   */
  historySyncUrl?: string | null;
  /**
   * Disable device sync
   */
  disableDeviceSync?: boolean;
};

/**
 * Storage options
 */
export type StorageOptions = {
  /**
   * Path to the local DB
   *
   * There are 4 value types that can be used to specify the database path:
   *
   * - `undefined` (or excluded from the client options)
   *    The database will be created in the current working directory and is based on
   *    the XMTP environment and client inbox ID.
   *    Example: `xmtp-dev-<inbox-id>.db3`
   *
   * - `null`
   *    No database will be created and all data will be lost once the client disconnects.
   *
   * - `string`
   *    The given path will be used to create the database.
   *    Example: `./my-db.db3`
   *
   * - `function`
   *    A callback function that receives the inbox ID and returns a string path.
   *    Example: `(inboxId) => string`
   */
  dbPath?: string | null | ((inboxId: string) => string);
  /**
   * Encryption key for the local DB (32 bytes, hex)
   *
   * @see https://docs.xmtp.org/chat-apps/core-messaging/create-a-client#view-an-encrypted-database
   */
  dbEncryptionKey?: Uint8Array | HexString;
  /**
   * Maximum number of connections in the local DB connection pool.
   *
   * Defaults to 25 when unset. Ignored when `useSingleConnection` is `true`.
   */
  maxDbPoolSize?: number;
  /**
   * Minimum number of connections kept warm in the local DB connection pool.
   *
   * Defaults to 5 when unset. Ignored when `useSingleConnection` is `true`.
   */
  minDbPoolSize?: number;
  /**
   * When `true`, the native DB uses a single connection (one file descriptor)
   * instead of a pool. The pool-size options above are ignored. Intended for
   * services running many clients in one process.
   *
   * Defaults to `false` (pooled).
   */
  useSingleConnection?: boolean;
};

export type ContentOptions = {
  /**
   * Allow configuring codecs for additional content types
   */
  codecs?: ContentCodec[];
};

export type OtherOptions = {
  /**
   * Enable structured JSON logging
   */
  structuredLogging?: boolean;
  /**
   * Logging level. Also the level exported to OTLP when `otelEndpoint` is set.
   */
  loggingLevel?: LogLevel;
  /**
   * Level for the stdout console layer only. Defaults to `loggingLevel`. Set to
   * `LogLevel.Warn` to quiet stdout below the OTLP export level — e.g. so a log
   * shipper does not duplicate logs already exported via OTLP, while OTLP still
   * receives `loggingLevel`.
   */
  stdoutLoggingLevel?: LogLevel;
  /**
   * OTLP endpoint (e.g. `"http://collector:4317"`) for exporting telemetry
   * spans and logs. When set, spans (and `tracing` events as correlated logs)
   * are exported via OTLP to this endpoint, where a downstream OpenTelemetry
   * Collector can derive metrics from the spans and forward the logs.
   *
   * Call {@link flushTelemetry} on graceful shutdown to flush buffered spans.
   */
  otelEndpoint?: string;
  /**
   * Resource attributes attached to all exported telemetry spans
   * (e.g. `{ "service.instance.id": "herald-7", "deployment.environment": "prod" }`).
   * Use these to attribute telemetry to its source.
   */
  resourceAttributes?: Record<string, string>;
  /**
   * Tuning for the background worker scheduler (intervals, jitter, per-worker
   * overrides, and disabled workers). All fields are optional; omitting this
   * object preserves the default worker behavior.
   *
   * Intervals are specified in nanoseconds.
   */
  workerConfig?: WorkerConfigOptions;
  /**
   * Disable automatic registration when creating a client
   */
  disableAutoRegister?: boolean;
  /**
   * The nonce to use when generating an inbox ID
   * (default: undefined = 1)
   */
  nonce?: bigint;
  /**
   * Options for waiting until client registration is visible on the network.
   *
   * When set, `registerIdentity` will wait for the specified quorum of nodes
   * to confirm the registration before resolving.
   */
  waitForRegistrationVisible?: VisibilityConfirmationOptions;
};

export type ClientOptions = (NetworkOptions | { backend: Backend }) &
  DeviceSyncOptions &
  StorageOptions &
  ContentOptions &
  OtherOptions;

/**
 * `Omit` that distributes over unions. The built-in `Omit` collapses a union
 * (e.g. `ClientOptions`' `NetworkOptions | { backend }` arm) because
 * `keyof (A | B)` only yields shared keys. This preserves each arm, so options
 * like `{ backend }` survive `Omit<ClientOptions, "codecs">`.
 */
export type DistributiveOmit<T, K extends PropertyKey> = T extends unknown
  ? Omit<T, K>
  : never;

export type EnrichedReply<T = unknown, U = unknown> = {
  referenceId: string;
  content: T;
  contentType: ContentTypeId | undefined;
  inReplyTo: DecodedMessage<U> | null;
};

export type BuiltInContentTypes =
  | string // text, markdown
  | LeaveRequest
  | Reaction
  | ReadReceipt
  | Attachment
  | RemoteAttachment
  | TransactionReference
  | WalletSendCalls
  | Actions
  | Intent
  | MultiRemoteAttachment
  | GroupUpdated
  | DeletedMessage;

export type ExtractCodecContentTypes<C extends ContentCodec[] = []> =
  C extends readonly []
    ? BuiltInContentTypes
    : [...C][number] extends ContentCodec<infer T>
      ?
          | T
          | BuiltInContentTypes
          | EnrichedReply<T | BuiltInContentTypes, T | BuiltInContentTypes>
      : BuiltInContentTypes;
