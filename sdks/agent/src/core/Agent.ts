import EventEmitter from "node:events";
import fs from "node:fs";
import path from "node:path";

import type { Dm, Group } from "@xmtp/node-sdk";
import {
  Client,
  ConversationStream,
  MessageStream,
  initLogging,
  type AnyContentCodec,
  type Actions,
  type Attachment,
  type ClientOptions,
  type Conversation,
  type CreateDmOptions,
  type CreateGroupOptions,
  type Message,
  type MessageContent,
  type GroupUpdated,
  type Intent,
  type LeaveRequest,
  type MultiRemoteAttachment,
  type Reaction,
  type RemoteAttachment,
  type StreamOptions,
  type TransactionReference,
  type WalletSendCalls,
} from "@xmtp/node-sdk";
import { isHex, toBytes, type Hex } from "viem";

import { filter } from "@/core/filter";
import { getInstallationInfo } from "@/debug";
import { parseLogLevel } from "@/debug/log";
import { createSigner, createUser } from "@/user/User";
import { version as appVersion } from "~/package.json";

import { AgentError, AgentStreamingError } from "./AgentError";
import { ClientContext } from "./ClientContext";
import { ConversationContext } from "./ConversationContext";
import { MessageContext } from "./MessageContext";

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
  dm: [ctx: ConversationContext<ContentTypes, Dm>];
  /** Group conversation event. */
  group: [ctx: ConversationContext<ContentTypes, Group>];
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
  "read-receipt": [ctx: MessageContext<undefined, ContentTypes>];
  /** Reply event. */
  reply: [
    ctx: MessageContext<
      Extract<
        MessageContent,
        {
          /** Select the reply content variant. */
          kind: "reply";
        }
      >,
      ContentTypes
    >,
  ];
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
type EthAddress = Hex;

/** Values available to a handler for the current message. */
export type AgentBaseContext<_ContentTypes = unknown> = {
  /** The client that received the message. */
  client: Client;
  /** The conversation that contains the message. */
  conversation: Conversation;
  /** The decoded message being handled. */
  message: Message;
};

/** Context passed to error middleware; message and conversation may be absent. */
export type AgentErrorContext<ContentTypes = unknown> = Partial<
  AgentBaseContext<ContentTypes>
> & {
  /** The client associated with the error. */
  client: Client;
};

/** Inputs used to wrap an already-created XMTP client. */
export type AgentOptions<_ContentTypes> = {
  /** Client to wrap. */
  client: Client;
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

/** Handle an error. Call `next()` to resume, or return to end the reader. */
export type AgentErrorMiddleware<ContentTypes = unknown> = (
  error: unknown,
  ctx: AgentErrorContext<ContentTypes>,
  next: (err?: unknown) => Promise<void> | void,
) => Promise<void> | void;

/** Client options used by `Agent.create`; `appVersion` and device sync have defaults. */
export type AgentCreateOptions<
  ContentCodecs extends readonly AnyContentCodec[] = [],
> = Omit<ClientOptions, "codecs"> & {
  /** Custom content codecs registered with the client. */
  readonly codecs?: ContentCodecs;
};

/** Options for both supported stream pumps. */
export type AgentStreamingOptions = StreamOptions;
/** Options for the agent message stream. */
export type StreamAllMessagesOptions<_ContentTypes> = StreamOptions;

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

type ErrorDisposition = "resume" | "stop" | "unhandled";

class UnacceptedValueError extends Error {
  constructor(readonly valueError: unknown) {
    super("Agent value processing failed without acceptance.");
  }
}

/** Event-driven XMTP agent that routes conversations and messages to middleware. */
export class Agent<ContentTypes = unknown> extends EventEmitter<
  EventHandlerMap<ContentTypes>
> {
  #client: Client;
  #conversationsStream?: ConversationStream;
  #messageStream?: MessageStream;
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
  #streamGeneration = 0;
  #closingStreams?: Promise<void>;

  /** Wrap an existing client without starting streams. */
  constructor({ client }: AgentOptions<ContentTypes>) {
    super();
    this.#client = client;
  }

  /** Create an agent and client. `backend.credentials` supplies backend credentials. Device sync defaults to disabled. */
  static async create<ContentCodecs extends readonly AnyContentCodec[] = []>(
    signer: Parameters<typeof Client.create>[0],
    // Note: we need to omit this so that "Client.create" can correctly infer the codecs.
    options: AgentCreateOptions<ContentCodecs>,
  ) {
    const backend =
      options.backend && "url" in options.backend
        ? {
            ...options.backend,
            appVersion: options.backend.appVersion ?? `agent-sdk/${appVersion}`,
          }
        : options.backend;
    const initializedOptions = {
      ...options,
      backend,
      deviceSync: options.deviceSync ?? false,
    };
    if (process.env.XMTP_FORCE_DEBUG_LEVEL) {
      const level = parseLogLevel(process.env.XMTP_FORCE_DEBUG_LEVEL);
      await initLogging({ level: level ?? "warn", structured: true });
    }
    const client = await Client.create(signer, initializedOptions);

    const info = await getInstallationInfo(client);
    if (info.totalInstallations > 1 && info.isMostRecent) {
      console.warn(
        `[WARNING] You have "${info.totalInstallations}" installations. Installation ID "${info.installationId}" is the most recent. Make sure to persist and reload your installation data. If you exceed the installation limit, your Agent will stop working. Read more: https://docs.xmtp.org/agents/build-agents/local-database#installation-limits-and-revocation-rules`,
      );
    }

    return new Agent({ client });
  }

  /** Create an agent from `XMTP_*` variables. `XMTP_BACKEND_URL` overrides `options.backend`; one must be supplied. Pass `backend.credentials` in options for backend authentication. */
  static async createFromEnv<
    ContentCodecs extends readonly AnyContentCodec[] = [],
  >(
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
    if (!XMTP_WALLET_KEY || !isHex(XMTP_WALLET_KEY, { strict: true }))
      throw new AgentError(1000, "XMTP_WALLET_KEY must be a hexadecimal key.");
    const backend = XMTP_BACKEND_URL
      ? {
          ...(options?.backend && "url" in options.backend
            ? options.backend
            : {}),
          url: XMTP_BACKEND_URL,
        }
      : options?.backend;
    if (!backend)
      throw new AgentError(
        1000,
        "XMTP_BACKEND_URL or options.backend is required.",
      );
    if (XMTP_DB_DIRECTORY && !options?.storage)
      fs.mkdirSync(XMTP_DB_DIRECTORY, { recursive: true, mode: 0o700 });
    let storage = options?.storage;
    if (!storage) {
      const legacyDirectory = XMTP_DB_DIRECTORY || process.cwd();
      const legacyFiles = fs
        .readdirSync(legacyDirectory, { withFileTypes: true })
        .filter((entry) => {
          if (!entry.isFile()) return false;
          if (XMTP_DB_DIRECTORY)
            return /^xmtp-[0-9a-f]{64}\.db3$/i.test(entry.name);
          const match = /^xmtp-(.+)-[0-9a-f]{64}\.db3$/i.exec(entry.name);
          return (
            match !== null && (XMTP_ENV === undefined || match[1] === XMTP_ENV)
          );
        })
        .map((entry) => path.join(legacyDirectory, entry.name));
      if (legacyFiles.length > 1)
        throw new AgentError(
          1000,
          "More than one legacy XMTP database exists. Pass an explicit storage location.",
        );
      const legacyPath = legacyFiles[0];
      storage = legacyPath
        ? {
            location: {
              dbPath: legacyPath,
              attachmentsDir: `${legacyPath}.attachments`,
            },
          }
        : {
            location: XMTP_DB_DIRECTORY
              ? { directory: XMTP_DB_DIRECTORY }
              : "default",
            label: XMTP_ENV,
          };
    }
    const key =
      options?.storage?.encryptionKey === undefined
        ? XMTP_DB_ENCRYPTION_KEY?.replace(/^0x/, "")
        : undefined;
    if (key && !/^[0-9a-fA-F]{64}$/.test(key))
      throw new AgentError(
        1000,
        "XMTP_DB_ENCRYPTION_KEY must contain 32 bytes.",
      );
    return this.create(createSigner(createUser(XMTP_WALLET_KEY)), {
      ...options,
      backend,
      storage: {
        ...storage,
        ...(key ? { encryptionKey: toBytes(`0x${key}`) } : {}),
      },
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
    // Detach before awaiting close. Old cleanup cannot clear a new stream.
    const conversations = this.#conversationsStream;
    const messages = this.#messageStream;
    this.#conversationsStream = undefined;
    this.#messageStream = undefined;
    const previous = this.#closingStreams;
    const closing = (async () => {
      const results = await Promise.allSettled([
        previous,
        Promise.resolve().then(() => conversations?.end()),
        Promise.resolve().then(() => messages?.end()),
      ]);
      for (const result of results) {
        if (result.status === "rejected") throw result.reason;
      }
    })();
    this.#closingStreams = closing;
    try {
      await closing;
    } finally {
      if (this.#closingStreams === closing) this.#closingStreams = undefined;
    }
  }

  /** End this stream generation. Only an explicit start opens a new budget. */
  async #handleStreamError(error: unknown, generation: number) {
    if (generation !== this.#streamGeneration) return;
    const stoppedGeneration = ++this.#streamGeneration;
    this.#isLocked = true;
    try {
      await this.#stopStreams();
    } catch {
      // Keep the stream failure as the cause presented to the application.
    }
    if (stoppedGeneration !== this.#streamGeneration) return;
    this.#isLocked = false;
    // Error middleware can explicitly start a fresh generation here. A
    // handled error alone does not silently renew an exhausted retry budget.
    if (!(error instanceof UnacceptedValueError))
      await this.#runErrorChain(error, { client: this.#client });
  }

  async #setupStreams(generation: number, options?: AgentStreamingOptions) {
    const isCurrent = () => generation === this.#streamGeneration;
    const close = (
      reason: Parameters<NonNullable<StreamOptions["onClose"]>>[0],
    ) => {
      try {
        options?.onClose?.(reason);
      } finally {
        // App callback failure must not retain this generation's readers.
        // Reentrant cleanup can already have started a new generation.
        if (isCurrent()) {
          if (reason.kind === "failed") {
            void this.#handleStreamError(
              reason.error instanceof UnacceptedValueError
                ? reason.error
                : new AgentStreamingError(
                    1004,
                    "Agent stream failed.",
                    reason.error,
                  ),
              generation,
            );
          } else {
            void this.stop().catch((error) =>
              this.#runErrorChain(error, { client: this.#client }),
            );
          }
        }
      }
    };
    const conversations = ConversationStream.open(this.#client, undefined, {
      ...options,
      onClose: close,
    });
    this.#conversationsStream = conversations;
    await conversations.ready();
    if (!isCurrent()) return false;
    void conversations
      .onValue(async (conversation) => {
        if (!isCurrent()) return;
        try {
          const context = new ConversationContext<ContentTypes>({
            conversation,
            client: this.#client,
          });
          this.emit("conversation", context);
          if (!isCurrent()) return;
          if (context.isGroup()) this.emit("group", context);
          else if (context.isDm()) this.emit("dm", context);
        } catch (error) {
          if (error instanceof UnacceptedValueError) throw error;
          if (isCurrent()) {
            const disposition = await this.#runErrorChain(error, {
              client: this.#client,
              conversation,
            });
            if (disposition !== "resume") throw new UnacceptedValueError(error);
          }
        }
      })
      .catch((error) => this.#handleStreamError(error, generation));
    const messages = MessageStream.open(this.#client, undefined, {
      ...options,
      onClose: close,
    });
    this.#messageStream = messages;
    await messages.ready();
    if (!isCurrent()) return false;
    const topics: Partial<
      Record<MessageContent["kind"], EventName<ContentTypes>>
    > = {
      actions: "actions",
      attachment: "inline-attachment",
      intent: "intent",
      groupUpdated: "group-update",
      leaveRequest: "leave-request",
      multiRemoteAttachment: "multi-attachment",
      remoteAttachment: "attachment",
      reaction: "reaction",
      readReceipt: "read-receipt",
      reply: "reply",
      transactionReference: "transaction-reference",
      walletSendCalls: "wallet-send-calls",
      markdown: "markdown",
      text: "text",
    };
    void messages
      .onValue(async (message) => {
        try {
          await this.#processMessage(
            message,
            isCurrent,
            topics[message.content.kind] ?? "unknownMessage",
          );
        } catch (error) {
          if (error instanceof UnacceptedValueError) throw error;
          if (isCurrent()) {
            const disposition = await this.#runErrorChain(error, {
              client: this.#client,
              message,
            });
            if (disposition !== "resume") throw new UnacceptedValueError(error);
          }
        }
      })
      .catch((error) => this.#handleStreamError(error, generation));
    return true;
  }

  /** Start conversation and message streams. Calling this while running is a no-op. */
  async start(options?: AgentStreamingOptions) {
    if (this.#isLocked || this.#conversationsStream || this.#messageStream)
      return;

    const generation = ++this.#streamGeneration;
    this.#isLocked = true;

    try {
      if (
        (await this.#setupStreams(generation, options)) &&
        generation === this.#streamGeneration
      ) {
        this.#isLocked = false;
        this.emit("start", new ClientContext({ client: this.#client }));
      }
    } catch (error) {
      await this.#handleStreamError(
        new AgentStreamingError(
          1005,
          "Error occurred during stream setup.",
          error,
        ),
        generation,
      );
    }
  }

  async #processMessage(
    message: Message,
    isCurrent: () => boolean,
    topic: EventName<ContentTypes> = "unknownMessage",
  ) {
    // Skip messages from agent itself
    if (filter.fromSelf(message, this.#client)) {
      return;
    }

    const conversation = await this.#client.conversations.getById(
      message.conversationId,
    );
    if (!isCurrent()) return;

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
    await this.#runMiddlewareChain(context, topic, isCurrent);
  }

  async #runMiddlewareChain(
    context: MessageContext<unknown, ContentTypes>,
    topic: EventName<ContentTypes>,
    isCurrent: () => boolean,
  ) {
    const finalEmit = async () => {
      if (!isCurrent()) return;
      try {
        this.emit(topic, context);
        if (!isCurrent()) return;
        this.emit("message", context);
      } catch (error) {
        if (error instanceof UnacceptedValueError) throw error;
        if (isCurrent()) {
          const disposition = await this.#runErrorChain(error, context);
          if (disposition !== "resume") throw new UnacceptedValueError(error);
        }
      }
    };

    const chain = this.#middleware.reduceRight<Parameters<AgentMiddleware>[1]>(
      (next, mw) => {
        return async () => {
          if (!isCurrent()) return;
          try {
            await mw(context, next);
          } catch (error) {
            if (error instanceof UnacceptedValueError) throw error;
            if (!isCurrent()) return;
            const disposition = await this.#runErrorChain(error, context);
            if (disposition === "resume" && isCurrent()) {
              await next();
            }
            if (disposition !== "resume") throw new UnacceptedValueError(error);
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
  ): Promise<ErrorDisposition> {
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
          return "resume";
        case "stopped":
          // A custom handler can stop the chain. The default handler cannot
          // accept an unhandled value on behalf of the application.
          return handler === this.#defaultErrorHandler ? "unhandled" : "stop";
        case "continue":
          // Error is passed to the next handler
          currentError = outcome.error;
      }
    }

    // Reached end of chain without recovery
    return "unhandled";
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
    const generation = ++this.#streamGeneration;
    this.#isLocked = true;

    try {
      await this.#stopStreams();
    } finally {
      if (generation === this.#streamGeneration) this.#isLocked = false;
    }
    this.emit("stop", new ClientContext({ client: this.#client }));
  }

  /** Create a DM with an Ethereum address. The address is converted to an identifier. */
  createDmWithAddress(address: EthAddress, options?: CreateDmOptions) {
    return this.#client.conversations.createDm(
      {
        identifier: address,
        kind: "ethereum" as const,
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
        kind: "ethereum" as const,
      };
    });
    return this.#client.conversations.createGroup(identifiers, options);
  }

  /** Add Ethereum addresses to an existing group. */
  addMembersWithAddresses<_ContentTypes>(
    group: Group,
    addresses: EthAddress[],
  ): ReturnType<Group["addMembers"]> {
    const identifiers = addresses.map((address) => {
      return {
        identifier: address,
        kind: "ethereum" as const,
      };
    });

    return group.addMembers(identifiers);
  }

  /** Resolve a conversation context, or return `undefined` when it is not local. */
  async getConversationContext(conversationId: string) {
    const conversation =
      await this.client.conversations.getById(conversationId);
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
    return this.#client.identity.identifier;
  }
}
