import EventEmitter from "node:events";
import fs from "node:fs";
import path from "node:path";
import type { ContentCodec } from "@xmtp/content-type-primitives";
import {
  Client,
  Dm,
  Group,
  IdentifierKind,
  isActions,
  isAttachment,
  isGroupUpdated,
  isHexString,
  isIntent,
  isLeaveRequest,
  isMarkdown,
  isMultiRemoteAttachment,
  isReaction,
  isReadReceipt,
  isRemoteAttachment,
  isReply,
  isText,
  isTransactionReference,
  isWalletSendCalls,
  LogLevel,
  type Actions,
  type Attachment,
  type ClientOptions,
  type Conversation,
  type CreateDmOptions,
  type CreateGroupOptions,
  type DecodedMessage,
  type EnrichedReply,
  type GroupUpdated,
  type HexString,
  type Intent,
  type LeaveRequest,
  type MultiRemoteAttachment,
  type NetworkOptions,
  type Reaction,
  type ReadReceipt,
  type RemoteAttachment,
  type StreamOptions,
  type TransactionReference,
  type WalletSendCalls,
} from "@xmtp/node-sdk";
import { version as appVersion } from "~/package.json";
import { retry } from "ts-retry-promise";
import { filter } from "@/core/filter";
import { getInstallationInfo } from "@/debug";
import { getValidLogLevels, parseLogLevel } from "@/debug/log";
import { createSigner, createUser } from "@/user/User";
import { AgentError, AgentStreamingError } from "./AgentError";
import { ClientContext } from "./ClientContext";
import { ConversationContext } from "./ConversationContext";
import { MessageContext } from "./MessageContext";

type ConversationStream<ContentTypes> = Awaited<
  ReturnType<Client<ContentTypes>["conversations"]["stream"]>
>;

type MessageStream<ContentTypes> = Awaited<
  ReturnType<Client<ContentTypes>["conversations"]["streamAllMessages"]>
>;

/** Event names and handler arguments emitted by an agent. */
export type EventHandlerMap<ContentTypes> = {
  /** Actions message event. */
  actions: [ctx: MessageContext<Actions, ContentTypes>];
  /** Remote attachment message event. */
  attachment: [ctx: MessageContext<RemoteAttachment, ContentTypes>];
  /** Any conversation event. */
  conversation: [ctx: ConversationContext<ContentTypes>];
  /** Group update event. */
  "group-update": [ctx: MessageContext<GroupUpdated, ContentTypes>];
  /** Direct-message conversation event. */
  dm: [ctx: ConversationContext<ContentTypes, Dm<ContentTypes>>];
  /** Group conversation event. */
  group: [ctx: ConversationContext<ContentTypes, Group<ContentTypes>>];
  /** Inline attachment event. */
  "inline-attachment": [ctx: MessageContext<Attachment, ContentTypes>];
  /** Intent event. */
  intent: [ctx: MessageContext<Intent, ContentTypes>];
  /** Leave request event. */
  "leave-request": [ctx: MessageContext<LeaveRequest, ContentTypes>];
  /** Markdown message event. */
  markdown: [ctx: MessageContext<string, ContentTypes>];
  /** Generic message event. */
  message: [ctx: MessageContext<unknown, ContentTypes>];
  /** Multiple attachment event. */
  "multi-attachment": [
    ctx: MessageContext<MultiRemoteAttachment, ContentTypes>,
  ];
  /** Reaction message event. */
  reaction: [ctx: MessageContext<Reaction, ContentTypes>];
  /** Read receipt event. */
  "read-receipt": [ctx: MessageContext<ReadReceipt, ContentTypes>];
  /** Reply event. */
  reply: [ctx: MessageContext<EnrichedReply, ContentTypes>];
  /** Agent start event. */
  start: [ctx: ClientContext<ContentTypes>];
  /** Agent stop event. */
  stop: [ctx: ClientContext<ContentTypes>];
  /** Plain-text message event. */
  text: [ctx: MessageContext<string, ContentTypes>];
  /** Transaction reference event. */
  "transaction-reference": [
    ctx: MessageContext<TransactionReference, ContentTypes>,
  ];
  /** Error that was not handled by error middleware. */
  unhandledError: [error: Error];
  /** Undecodable or unsupported message event. */
  unknownMessage: [ctx: MessageContext<unknown, ContentTypes>];
  /** Wallet send calls event. */
  "wallet-send-calls": [ctx: MessageContext<WalletSendCalls, ContentTypes>];
};

type EventName<ContentTypes> = keyof EventHandlerMap<ContentTypes>;

/** Ethereum address encoded as a prefixed hexadecimal string. */
type EthAddress = HexString;

/** Values available to a handler for the current message. */
export type AgentBaseContext<ContentTypes = unknown> = {
  /** The client that received the message. */
  client: Client<ContentTypes>;
  /** The conversation that contains the message. */
  conversation: Conversation;
  /** The decoded message being handled. */
  message: DecodedMessage;
};

/** Context passed to error middleware; message and conversation may be absent. */
export type AgentErrorContext<ContentTypes = unknown> = Partial<
  AgentBaseContext<ContentTypes>
> & {
  /** The client associated with the error. */
  client: Client<ContentTypes>;
};

/** Inputs used to wrap an already-created XMTP client. */
export type AgentOptions<ContentTypes> = {
  /** Client to wrap. */
  client: Client<ContentTypes>;
};

/** Handles a decoded message in normal middleware or command routing. */
export type AgentMessageHandler<ContentTypes = unknown> = (
  ctx: MessageContext<ContentTypes>,
) => Promise<void> | void;

/** Processes a message and calls `next` to continue the middleware chain. */
export type AgentMiddleware<ContentTypes = unknown> = (
  ctx: MessageContext<unknown, ContentTypes>,
  next: () => Promise<void> | void,
) => Promise<void>;

/** Handles an error and calls `next` with no argument to resume processing. */
export type AgentErrorMiddleware<ContentTypes = unknown> = (
  error: unknown,
  ctx: AgentErrorContext<ContentTypes>,
  next: (err?: unknown) => Promise<void> | void,
) => Promise<void> | void;

/** Client options used by `Agent.create`; `appVersion` and device sync have defaults. */
export type AgentCreateOptions<ContentCodecs extends ContentCodec[] = []> =
  Omit<ClientOptions & NetworkOptions, "codecs"> & {
    /** Custom content codecs registered with the client. */
    codecs?: ContentCodecs;
  };

/** Stream options passed to both the conversation and message streams. */
export type AgentStreamingOptions = Omit<StreamOptions, "onValue" | "onError">;

/** Message-stream options exposed for callers that need the Node SDK shape. */
export type StreamAllMessagesOptions<ContentTypes> = Parameters<
  Client<ContentTypes>["conversations"]["streamAllMessages"]
>[0];

/** Registration API returned by `agent.errors`. */
export type AgentErrorRegistrar<ContentTypes> = {
  /** Append one or more error middleware functions to the error chain. */
  use(
    ...errorMiddleware: Array<
      AgentErrorMiddleware<ContentTypes> | AgentErrorMiddleware<ContentTypes>[]
    >
  ): AgentErrorRegistrar<ContentTypes>;
};

type ErrorFlow =
  | { kind: "handled" } // next()
  | { kind: "continue"; error: unknown } // next(err) or handler throws
  | { kind: "stopped" }; // handler returns without next()

/** Event-driven XMTP agent that routes conversations and messages to middleware. */
export class Agent<ContentTypes = unknown> extends EventEmitter<
  EventHandlerMap<ContentTypes>
> {
  #client: Client<ContentTypes>;
  #conversationsStream?: ConversationStream<ContentTypes>;
  #messageStream?: MessageStream<ContentTypes>;
  #middleware: AgentMiddleware<ContentTypes>[] = [];
  #errorMiddleware: AgentErrorMiddleware<ContentTypes>[] = [];
  #errors: AgentErrorRegistrar<ContentTypes> = Object.freeze({
    use: (...errorMiddleware: AgentErrorMiddleware<ContentTypes>[]) => {
      for (const emw of errorMiddleware) {
        if (Array.isArray(emw)) {
          this.#errorMiddleware.push(...emw);
        } else if (typeof emw === "function") {
          this.#errorMiddleware.push(emw);
        }
      }
      return this.#errors;
    },
  });
  #defaultErrorHandler: AgentErrorMiddleware<ContentTypes> = (
    currentError,
    _ctx,
    next,
  ) => {
    const emittedError =
      currentError instanceof Error
        ? currentError
        : new AgentError(
            9999,
            `Unhandled error caught by default error middleware.`,
            currentError,
          );
    this.emit("unhandledError", emittedError);
    if (currentError instanceof AgentStreamingError) {
      void next();
    }
  };
  #isLocked: boolean = false;
  #isRestarting: boolean = false;
  #stopped: boolean = false;
  #streamOptions?: AgentStreamingOptions;

  /** Wrap an existing client without starting streams. */
  constructor({ client }: AgentOptions<ContentTypes>) {
    super();
    this.#client = client;
  }

  /** Create an agent and client. Device sync defaults to disabled for agents. */
  static async create<ContentCodecs extends ContentCodec[] = []>(
    signer: Parameters<typeof Client.create>[0],
    // Note: we need to omit this so that "Client.create" can correctly infer the codecs.
    options: AgentCreateOptions<ContentCodecs>,
  ) {
    const initializedOptions = { ...options };
    initializedOptions.appVersion ??= `agent-sdk/${appVersion}`;
    initializedOptions.disableDeviceSync ??= true;

    if (process.env.XMTP_FORCE_DEBUG_LEVEL) {
      const rawLevel = process.env.XMTP_FORCE_DEBUG_LEVEL;
      const logLevel = parseLogLevel(rawLevel);

      if (logLevel) {
        initializedOptions.loggingLevel = logLevel;
      } else {
        console.warn(
          `[WARNING] Invalid XMTP_FORCE_DEBUG_LEVEL "${rawLevel}". Defaulting to "${LogLevel.Warn}". Valid values are: ${getValidLogLevels().join(", ")}`,
        );
        initializedOptions.loggingLevel = LogLevel.Warn;
      }
      initializedOptions.structuredLogging = true;
    }

    const client = await Client.create(signer, {
      ...initializedOptions,
      codecs: initializedOptions.codecs,
    });

    const info = await getInstallationInfo(client);
    if (info.totalInstallations > 1 && info.isMostRecent) {
      console.warn(
        `[WARNING] You have "${info.totalInstallations}" installations. Installation ID "${info.installationId}" is the most recent. Make sure to persist and reload your installation data. If you exceed the installation limit, your Agent will stop working. Read more: https://docs.xmtp.org/agents/build-agents/local-database#installation-limits-and-revocation-rules`,
      );
    }

    return new Agent({ client });
  }

  /** Create an agent from `XMTP_*` variables. `XMTP_BACKEND_URL` overrides `options.backendUrl`; one must be supplied. */
  static async createFromEnv<ContentCodecs extends ContentCodec[] = []>(
    // Note: we need to omit this so that "Client.create" can correctly infer the codecs.
    options?: Partial<AgentCreateOptions<ContentCodecs>>,
  ) {
    const {
      XMTP_DB_DIRECTORY,
      XMTP_DB_ENCRYPTION_KEY,
      XMTP_ENV,
      XMTP_WALLET_KEY,
      XMTP_BACKEND_URL,
    } = process.env;

    if (!isHexString(XMTP_WALLET_KEY)) {
      throw new AgentError(
        1000,
        `XMTP_WALLET_KEY env is not in hex (0x) format.`,
      );
    }

    const signer = createSigner(createUser(XMTP_WALLET_KEY));

    const initializedOptions = { ...options };

    initializedOptions.dbEncryptionKey =
      typeof XMTP_DB_ENCRYPTION_KEY === "string"
        ? isHexString(XMTP_DB_ENCRYPTION_KEY)
          ? XMTP_DB_ENCRYPTION_KEY
          : `0x${XMTP_DB_ENCRYPTION_KEY}`
        : undefined;

    if (XMTP_ENV !== undefined) {
      initializedOptions.env = XMTP_ENV;
    }

    if (typeof XMTP_BACKEND_URL === "string") {
      initializedOptions.backendUrl = XMTP_BACKEND_URL;
    }

    if (typeof XMTP_DB_DIRECTORY === "string") {
      fs.mkdirSync(XMTP_DB_DIRECTORY, { recursive: true, mode: 0o700 });
      initializedOptions.dbPath = (inboxId: string) => {
        const dbPath = path.join(XMTP_DB_DIRECTORY, `xmtp-${inboxId}.db3`);
        console.info(`Saving local database to "${dbPath}"`);
        return dbPath;
      };
    }

    if (!initializedOptions.backendUrl?.trim()) {
      throw new Error("backendUrl is required");
    }
    return this.create(signer, {
      ...initializedOptions,
      backendUrl: initializedOptions.backendUrl,
    });
  }

  /** Return the libxmtp version used by the wrapped client. */
  get libxmtpVersion() {
    return this.#client.libxmtpVersion;
  }

  /** Add message middleware. Middleware runs in registration order. */
  use(
    ...middleware: Array<
      AgentMiddleware<ContentTypes> | AgentMiddleware<ContentTypes>[]
    >
  ): this {
    for (const mw of middleware) {
      if (Array.isArray(mw)) {
        this.#middleware.push(...mw);
      } else if (typeof mw === "function") {
        this.#middleware.push(mw);
      }
    }
    return this;
  }

  async #stopStreams() {
    try {
      await this.#conversationsStream?.end();
    } finally {
      this.#conversationsStream = undefined;
    }

    try {
      await this.#messageStream?.end();
    } finally {
      this.#messageStream = undefined;
    }
  }

  /**
   * Closes all existing streams and restarts with exponential backoff.
   */
  async #handleStreamError(error: unknown) {
    if (this.#isRestarting) return;
    this.#isRestarting = true;

    await this.#stopStreams();

    const recovered = await this.#runErrorChain(error, {
      client: this.#client,
    });

    if (recovered && !this.#stopped) {
      await this.#retryStreams();
      this.emit("start", new ClientContext({ client: this.#client }));
      this.#isLocked = false;
    } else {
      this.#isLocked = false;
    }

    this.#isRestarting = false;
  }

  async #retryStreams() {
    return retry(
      async () => {
        await this.#stopStreams();
        await this.#setupStreams(this.#streamOptions);
      },
      {
        retries: 10,
        delay: 1000,
        backoff: "EXPONENTIAL",
        maxBackOff: 30_000,
        timeout: "INFINITELY",
        retryIf: () => !this.#stopped,
      },
    );
  }

  async #setupStreams(options?: AgentStreamingOptions) {
    this.#conversationsStream = await this.#client.conversations.stream({
      ...options,
      onValue: async (conversation) => {
        try {
          if (!conversation) {
            return;
          }
          this.emit(
            "conversation",
            new ConversationContext<ContentTypes, Conversation<ContentTypes>>({
              conversation,
              client: this.#client,
            }),
          );
          if (conversation instanceof Group) {
            this.emit(
              "group",
              new ConversationContext<ContentTypes, Group<ContentTypes>>({
                conversation,
                client: this.#client,
              }),
            );
          } else if (conversation instanceof Dm) {
            this.emit(
              "dm",
              new ConversationContext<ContentTypes, Dm<ContentTypes>>({
                conversation,
                client: this.#client,
              }),
            );
          }
        } catch (error) {
          const recovered = await this.#runErrorChain(
            new AgentError(
              1001,
              "Emitted value from conversation stream caused an error.",
              error,
            ),
            new ClientContext({ client: this.#client }),
          );
          if (!recovered) await this.stop();
        }
      },
      onError: async (error) => {
        await this.#handleStreamError(
          new AgentStreamingError(
            1002,
            "Error occurred during conversation streaming.",
            error,
          ),
        );
      },
    });

    this.#messageStream = await this.#client.conversations.streamAllMessages({
      ...options,
      onValue: async (message) => {
        try {
          switch (true) {
            case isActions(message):
              await this.#processMessage(message, "actions");
              break;
            case isAttachment(message):
              await this.#processMessage(message, "inline-attachment");
              break;
            case isIntent(message):
              await this.#processMessage(message, "intent");
              break;
            case isGroupUpdated(message):
              await this.#processMessage(message, "group-update");
              break;
            case isLeaveRequest(message):
              await this.#processMessage(message, "leave-request");
              break;
            case isMultiRemoteAttachment(message):
              await this.#processMessage(message, "multi-attachment");
              break;
            case isRemoteAttachment(message):
              await this.#processMessage(message, "attachment");
              break;
            case isReaction(message):
              await this.#processMessage(message, "reaction");
              break;
            case isReadReceipt(message):
              await this.#processMessage(message, "read-receipt");
              break;
            case isReply(message):
              await this.#processMessage(message, "reply");
              break;
            case isTransactionReference(message):
              await this.#processMessage(message, "transaction-reference");
              break;
            case isWalletSendCalls(message):
              await this.#processMessage(message, "wallet-send-calls");
              break;
            case isMarkdown(message):
              await this.#processMessage(message, "markdown");
              break;
            case isText(message):
              await this.#processMessage(message, "text");
              break;
            default:
              await this.#processMessage(message);
              break;
          }
        } catch (error) {
          const recovered = await this.#runErrorChain(error, {
            client: this.#client,
          });
          if (!recovered) {
            await this.stop();
          }
          this.#isLocked = false;
        }
      },
      onError: async (error) => {
        await this.#handleStreamError(
          new AgentStreamingError(
            1004,
            "Error occurred during message streaming.",
            error,
          ),
        );
      },
    });
  }

  /** Start conversation and message streams. Calling this while running is a no-op. */
  async start(options?: AgentStreamingOptions) {
    if (this.#isLocked || this.#conversationsStream || this.#messageStream)
      return;

    this.#stopped = false;
    this.#streamOptions = options;
    this.#isLocked = true;

    try {
      await this.#setupStreams(options);
      this.emit("start", new ClientContext({ client: this.#client }));
      this.#isLocked = false;
    } catch (error) {
      await this.#handleStreamError(
        new AgentStreamingError(
          1005,
          "Error occurred during stream setup.",
          error,
        ),
      );
    }
  }

  async #processMessage(
    message: DecodedMessage<ContentTypes>,
    topic: EventName<ContentTypes> = "unknownMessage",
  ) {
    // Skip messages with undefined content (failed to decode)
    if (!filter.hasContent(message)) {
      return;
    }

    // Skip messages from agent itself
    if (filter.fromSelf(message, this.#client)) {
      return;
    }

    const conversation = await this.#client.conversations.getConversationById(
      message.conversationId,
    );

    if (!conversation) {
      throw new AgentError(
        1003,
        `Failed to process message ID "${message.id}" for conversation ID "${message.conversationId}" because the conversation could not be found.`,
      );
    }

    const context = new MessageContext({
      message,
      conversation,
      client: this.#client,
    });
    await this.#runMiddlewareChain(context, topic);
  }

  async #runMiddlewareChain(
    context: MessageContext<unknown, ContentTypes>,
    topic: EventName<ContentTypes> = "unknownMessage",
  ) {
    const finalEmit = async () => {
      try {
        this.emit(topic, context);
        this.emit("message", context);
      } catch (error) {
        await this.#runErrorChain(error, context);
      }
    };

    const chain = this.#middleware.reduceRight<Parameters<AgentMiddleware>[1]>(
      (next, mw) => {
        return async () => {
          try {
            await mw(context, next);
          } catch (error) {
            const resume = await this.#runErrorChain(error, context);
            if (resume) {
              await next();
            }
            // Chain is not resuming, error is being swallowed
          }
        };
      },
      finalEmit,
    );

    await chain();
  }

  async #runErrorHandler(
    handler: AgentErrorMiddleware<ContentTypes>,
    context: AgentErrorContext<ContentTypes>,
    error: unknown,
  ): Promise<ErrorFlow> {
    let settled = false as boolean;
    let flow: ErrorFlow = { kind: "stopped" };

    const next = (nextErr?: unknown) => {
      if (settled) return;
      settled = true;
      flow =
        nextErr === undefined
          ? { kind: "handled" }
          : { kind: "continue", error: nextErr };
    };

    try {
      await handler(error, context, next);
      return flow;
    } catch (thrown) {
      if (settled) {
        return flow;
      }
      return { kind: "continue", error: thrown };
    }
  }

  async #runErrorChain(
    error: unknown,
    context: AgentErrorContext<ContentTypes>,
  ): Promise<boolean> {
    const chain = [...this.#errorMiddleware, this.#defaultErrorHandler];

    let currentError: unknown = error;

    for (let i = 0; i < chain.length; i++) {
      const handler = chain[i];
      if (!handler) continue;
      const outcome = await this.#runErrorHandler(
        handler,
        context,
        currentError,
      );

      switch (outcome.kind) {
        case "handled":
          // Error was handled. Main middleware can continue.
          return true;
        case "stopped":
          // Error cannot be handled. Main middleware won't continue.
          return false;
        case "continue":
          // Error is passed to the next handler
          currentError = outcome.error;
      }
    }

    // Reached end of chain without recovery
    return false;
  }

  /** Return the wrapped client for operations outside agent middleware. */
  get client() {
    return this.#client;
  }

  /** Return the error-middleware registrar. */
  get errors() {
    return this.#errors;
  }

  /** Stop both streams and emit the `stop` event. Calling this is safe repeatedly. */
  async stop() {
    this.#stopped = true;
    this.#isLocked = true;

    await this.#stopStreams();

    this.emit("stop", new ClientContext({ client: this.#client }));

    this.#isLocked = false;
  }

  /** Create a DM with an Ethereum address. The address is converted to an identifier. */
  createDmWithAddress(address: EthAddress, options?: CreateDmOptions) {
    return this.#client.conversations.createDmWithIdentifier(
      {
        identifier: address,
        identifierKind: IdentifierKind.Ethereum,
      },
      options,
    );
  }

  /** Create a group from Ethereum addresses. */
  createGroupWithAddresses(
    addresses: EthAddress[],
    options?: CreateGroupOptions,
  ) {
    const identifiers = addresses.map((address) => {
      return {
        identifier: address,
        identifierKind: IdentifierKind.Ethereum,
      };
    });
    return this.#client.conversations.createGroupWithIdentifiers(
      identifiers,
      options,
    );
  }

  /** Add Ethereum addresses to an existing group. */
  addMembersWithAddresses<ContentTypes>(
    group: Group<ContentTypes>,
    addresses: EthAddress[],
  ) {
    const identifiers = addresses.map((address) => {
      return {
        identifier: address,
        identifierKind: IdentifierKind.Ethereum,
      };
    });

    return group.addMembersByIdentifiers(identifiers);
  }

  /** Resolve a conversation context, or return `undefined` when it is not local. */
  async getConversationContext(conversationId: string) {
    const conversation =
      await this.client.conversations.getConversationById(conversationId);
    if (conversation) {
      const context = new ConversationContext({
        conversation,
        client: this.#client,
      });
      return context;
    }
  }

  /** Return the agent account address, when the client has one. */
  get address() {
    return this.#client.accountIdentifier?.identifier;
  }
}
