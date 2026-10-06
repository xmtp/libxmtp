import {
  currentProjection,
  liftConnectionState,
  liftConversation,
  lowerConversationMessageReaderOptions,
  lowerConversationReaderOptions,
  lowerMessageReaderOptions,
  publicError,
  unwrapConversations,
  unwrapDm,
  unwrapGroup,
  type ConnectionState,
  type Conversation,
  XmtpError,
  type ConversationKind,
  type ConsentState,
  type DeliveryCursor,
  type Conversations,
  type Dm,
  type Group,
} from "../../public-values.gen";
import type { ConnectionState as BoundState } from "../../xmtp_sdk";
import {
  ReaderStream,
  type ReaderLike,
  type StreamOptions as HostStreamOptions,
} from "../streams/reader";
import type { Client } from "./client";
import type { Message } from "./message";

/** Why a stream closed. A failure carries the public error. */
export type StreamCloseReason =
  | { readonly kind: "closed" }
  | { readonly kind: "failed"; readonly error: unknown };

export type StreamOptions = {
  readonly signal?: AbortSignal;
  readonly onClose?: (reason: StreamCloseReason) => void;
  readonly onConnectionStateChange?: (
    previous: ConnectionState | undefined,
    current: ConnectionState,
  ) => void;
};

async function rethrow<T>(operation: () => Promise<T>): Promise<T> {
  try {
    return await operation();
  } catch (error) {
    throw publicError(error);
  }
}

// The shared stream reads through this reader. Values and failures leave it
// as public values; connection states stay binding values until the options
// below lift them.
function publicReader<R, T>(
  reader: ReaderLike<R>,
  lift: (value: R) => T,
): ReaderLike<T> {
  const connectionState = reader.connectionState?.bind(reader);
  const connectionStateChanged = reader.connectionStateChanged?.bind(reader);
  return {
    next: (options) =>
      rethrow(async () => {
        const value = await reader.next(options);
        return value === undefined ? undefined : lift(value);
      }),
    end: () => rethrow(() => reader.end()),
    connectionState:
      connectionState === undefined
        ? undefined
        : () => rethrow(connectionState),
    connectionStateChanged:
      connectionStateChanged === undefined
        ? undefined
        : (previous) => rethrow(() => connectionStateChanged(previous)),
  };
}

/** The host options of public stream options. Exported for conformance. */
export function hostOptions(options: StreamOptions = {}): HostStreamOptions {
  const { signal, onClose, onConnectionStateChange } = options;
  return {
    signal,
    onClose,
    onConnectionStateChange:
      onConnectionStateChange === undefined
        ? undefined
        : (previous: BoundState | undefined, current: BoundState) => {
            const projection = currentProjection();
            onConnectionStateChange(
              previous === undefined
                ? undefined
                : liftConnectionState(previous, projection),
              liftConnectionState(current, projection),
            );
          },
  };
}

export type ConversationStreamOptions = StreamOptions & {
  readonly conversationKind?: ConversationKind;
  readonly consentStates?: readonly ConsentState[];
};
export type MessageStreamOptions = ConversationStreamOptions & {
  readonly from?: DeliveryCursor;
};
export type ConversationMessageStreamOptions = StreamOptions & {
  readonly from?: DeliveryCursor;
};

type StreamReceiver = { sdkStreamOwnerKey(): bigint };
let resolveOwner: (source: StreamReceiver) => Client | undefined;

/** Install the host lookup with the public projection. */
export function installStreamOwner(resolve: typeof resolveOwner): void {
  resolveOwner = resolve;
}

function receiverOwner(source: StreamReceiver): Client {
  const owner = resolveOwner(source);
  if (owner === undefined)
    throw new XmtpError.ClientClosed({
      code: "ClientClosed",
      category: "lifecycle",
      retryable: false,
      message: "client is closed",
    });
  return owner;
}

type StreamFactory<T, S> = (
  open: (signal: AbortSignal) => Promise<ReaderLike<T>>,
  client: Client,
  options?: StreamOptions,
) => S;
let messageStream: StreamFactory<Message, MessageStream>;
let conversationStream: StreamFactory<Conversation, ConversationStream>;

/**
 * Messages in delivery order. The stream holds its client while it is open.
 *
 * Without `options.from`, one default message reader owns delivery progress
 * for the client database. Another default reader fails with `ConsumerOwned`,
 * even if it selects a different group, DM, or filter.
 *
 * An explicit `options.from` cursor starts an independent replay/live reader.
 * Such readers can run in parallel and do not change default delivery progress.
 * Their progress is not a separate durable consumer checkpoint. Save the last
 * processed message's `deliveryCursor` if the app must resume this replay.
 *
 * Each next read acknowledges the prior message. Await processing before the
 * next read, or use `onValue()` and await all work in its callback. An app queue
 * or an unawaited task does not delay acknowledgement. `end()` and `return()`
 * do not acknowledge the last message. They cannot undo an acknowledgement after
 * its commit has been admitted.
 */
export class MessageStream extends ReaderStream<Message> {
  static {
    messageStream = (open, client, options) =>
      new MessageStream(open, client, options);
  }
  private constructor(
    open: (signal: AbortSignal) => Promise<ReaderLike<Message>>,
    client: Client,
    options?: StreamOptions,
  ) {
    super(open, client, hostOptions(options));
  }
}

/** Conversations as the client joins or creates them. */
export class ConversationStream extends ReaderStream<Conversation> {
  static {
    conversationStream = (open, client, options) =>
      new ConversationStream(open, client, options);
  }
  private constructor(
    open: (signal: AbortSignal) => Promise<ReaderLike<Conversation>>,
    client: Client,
    options?: StreamOptions,
  ) {
    super(open, client, hostOptions(options));
  }
}

export function openConversationStream(
  receiver: Conversations,
  options: ConversationStreamOptions = {},
): ConversationStream {
  const source = unwrapConversations(receiver);
  const client = receiverOwner(source);
  const projection = currentProjection();
  const selection = lowerConversationReaderOptions(
    {
      kind: options.conversationKind,
      consentStates:
        options.consentStates === undefined
          ? undefined
          : [...options.consentStates],
    },
    projection,
  );
  return conversationStream(
    async (signal) =>
      publicReader(
        await rethrow(() => source.conversationReader(selection, { signal })),
        (value) => liftConversation(value, projection),
      ),
    client,
    options,
  );
}

export function openAllMessages(
  receiver: Conversations,
  options: MessageStreamOptions = {},
): MessageStream {
  const source = unwrapConversations(receiver);
  const client = receiverOwner(source);
  const projection = currentProjection();
  const selection = lowerMessageReaderOptions(
    {
      conversationKind: options.conversationKind,
      consentStates:
        options.consentStates === undefined
          ? undefined
          : [...options.consentStates],
      from: options.from,
    },
    projection,
  );
  return messageStream(
    async (signal) =>
      publicReader(
        await rethrow(() => source.messageReader(selection, { signal })),
        (value) => projection.liftMessage(value),
      ),
    client,
    options,
  );
}

export function openGroupMessages(
  receiver: Group,
  options?: ConversationMessageStreamOptions,
): MessageStream {
  return openMessages(unwrapGroup(receiver), options);
}
export function openDmMessages(
  receiver: Dm,
  options?: ConversationMessageStreamOptions,
): MessageStream {
  return openMessages(unwrapDm(receiver), options);
}
function openMessages(
  source: Pick<
    ReturnType<typeof unwrapGroup>,
    "messageReader" | "sdkStreamOwnerKey"
  >,
  options: ConversationMessageStreamOptions = {},
): MessageStream {
  const client = receiverOwner(source);
  const projection = currentProjection();
  const selection = lowerConversationMessageReaderOptions(
    { from: options.from },
    projection,
  );
  return messageStream(
    async (signal) =>
      publicReader(
        await rethrow(() => source.messageReader(selection, { signal })),
        (value) => projection.liftMessage(value),
      ),
    client,
    options,
  );
}
