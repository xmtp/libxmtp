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
  type ConversationMessageReaderOptions,
  type ConversationReaderOptions,
  type Dm,
  type Group,
  type MessageReaderOptions,
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

/**
 * Messages in delivery order. The stream holds its client while it is open.
 *
 * Without `selection.from`, one default message reader owns delivery progress
 * for the client database. Another default reader fails with `ConsumerOwned`,
 * even if it selects a different group, DM, or filter.
 *
 * An explicit `selection.from` cursor starts an independent replay/live reader.
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
  /** All conversations, with fixed filters for the stream's lifetime. */
  static open(
    client: Client,
    selection?: MessageReaderOptions,
    options?: StreamOptions,
  ): MessageStream {
    const projection = currentProjection();
    const conversations = unwrapConversations(client.conversations);
    return new MessageStream(
      async (signal) =>
        publicReader(
          await rethrow(() =>
            conversations.messageReader(
              selection === undefined
                ? undefined
                : lowerMessageReaderOptions(selection, projection),
              { signal },
            ),
          ),
          (value) => projection.liftMessage(value),
        ),
      client,
      options,
    );
  }

  /** One group. */
  static openGroup(
    client: Client,
    group: Group,
    selection?: ConversationMessageReaderOptions,
    options?: StreamOptions,
  ): MessageStream {
    return MessageStream.conversation(
      client,
      unwrapGroup(group),
      selection,
      options,
    );
  }

  /** One DM, including its stitched groups. */
  static openDm(
    client: Client,
    dm: Dm,
    selection?: ConversationMessageReaderOptions,
    options?: StreamOptions,
  ): MessageStream {
    return MessageStream.conversation(client, unwrapDm(dm), selection, options);
  }

  private static conversation(
    client: Client,
    source: Pick<ReturnType<typeof unwrapGroup>, "messageReader">,
    selection: ConversationMessageReaderOptions | undefined,
    options: StreamOptions | undefined,
  ): MessageStream {
    const projection = currentProjection();
    return new MessageStream(
      async (signal) =>
        publicReader(
          await rethrow(() =>
            source.messageReader(
              selection === undefined
                ? undefined
                : lowerConversationMessageReaderOptions(selection, projection),
              { signal },
            ),
          ),
          (value) => projection.liftMessage(value),
        ),
      client,
      options,
    );
  }

  private constructor(
    open: (signal: AbortSignal) => Promise<ReaderLike<Message>>,
    client: Client,
    options: StreamOptions | undefined,
  ) {
    super(open, client, hostOptions(options));
  }
}

/** Conversations as the client joins or creates them. */
export class ConversationStream extends ReaderStream<Conversation> {
  static open(
    client: Client,
    selection?: ConversationReaderOptions,
    options?: StreamOptions,
  ): ConversationStream {
    const projection = currentProjection();
    const conversations = unwrapConversations(client.conversations);
    return new ConversationStream(
      async (signal) =>
        publicReader(
          await rethrow(() =>
            conversations.conversationReader(
              selection === undefined
                ? undefined
                : lowerConversationReaderOptions(selection, projection),
              { signal },
            ),
          ),
          (value) => liftConversation(value, projection),
        ),
      client,
      options,
    );
  }

  private constructor(
    open: (signal: AbortSignal) => Promise<ReaderLike<Conversation>>,
    client: Client,
    options: StreamOptions | undefined,
  ) {
    super(open, client, hostOptions(options));
  }
}
