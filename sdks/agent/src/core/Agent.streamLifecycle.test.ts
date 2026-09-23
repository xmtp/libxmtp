import { setImmediate } from "node:timers/promises";

import {
  Conversations as NodeConversations,
  ConversationType,
  MessageStream,
  type BuiltInContentTypes,
  type Client,
  type CodecRegistry,
  type DecodedMessage,
  type Group,
  type MessageReaderSource,
} from "@xmtp/node-sdk";
import { describe, expect, it, vi } from "vitest";

import { Agent, type AgentErrorMiddleware } from "@/core/Agent";
import { AgentStreamingError } from "@/core/AgentError";

type Conversations = Client["conversations"];
type ConversationOptions = Parameters<Conversations["stream"]>[0];
type MessageOptions = Parameters<Conversations["streamAllMessages"]>[0];

const harness = () => {
  const conversations: Array<{
    options: ConversationOptions;
    end: ReturnType<typeof vi.fn>;
  }> = [];
  const messages: Array<{
    options: MessageOptions;
    end: ReturnType<typeof vi.fn>;
  }> = [];
  const stream = vi.fn(async (options: ConversationOptions) => {
    const value = {
      options,
      end: vi.fn<() => Promise<void>>(async () => undefined),
    };
    conversations.push(value);
    return value;
  });
  const streamAllMessages = vi.fn(async (options: MessageOptions) => {
    const value = {
      options,
      end: vi.fn<() => Promise<void>>(async () => undefined),
    };
    messages.push(value);
    return value;
  });
  const client = {
    inboxId: "agent",
    conversations: { stream, streamAllMessages, getConversationById: vi.fn() },
  } as unknown as Client;
  return {
    agent: new Agent({ client }),
    client,
    conversations,
    messages,
    stream,
    streamAllMessages,
  };
};

const failNotification = (options: ConversationOptions, cause: Error) => {
  options?.onEnd?.();
  return Promise.resolve(options?.onError?.(cause));
};

describe("Agent stream lifecycle", () => {
  it("starts native recovery directly unless the caller requests pre-sync", async () => {
    const h = harness();
    await h.agent.start();
    expect(h.conversations[0]!.options?.disableSync).toBe(true);
    await h.agent.stop();
    await h.agent.start({ disableSync: undefined });
    expect(h.conversations[1]!.options?.disableSync).toBe(true);
    await h.agent.stop();
    await h.agent.start({ disableSync: false });
    expect(h.conversations[2]!.options?.disableSync).toBe(false);
    await h.agent.stop();
  });

  it("keeps the same streams through a retryable notification error", async () => {
    const h = harness();
    const callbacks: Array<(error: Error | null, value?: unknown) => void> = [];
    const closes: Array<() => void> = [];
    const nativeOpen = vi.fn(
      async (
        callback: (error: Error | null, value?: unknown) => void,
        onClose: () => void,
      ) => {
        callbacks.push(callback);
        closes.push(onClose);
        return {
          end: vi.fn(),
          waitForReady: vi.fn(async () => undefined),
        };
      },
    );
    const native = {
      stream: nativeOpen,
    } as unknown as ConstructorParameters<typeof NodeConversations>[2];
    const nodeConversations = new NodeConversations(
      h.client,
      {} as CodecRegistry,
      native,
    );
    h.stream.mockImplementation(async (options) => {
      const stream = await nodeConversations.stream(options);
      const value = {
        options,
        end: vi.fn(async () => {
          await stream.end();
        }),
      };
      h.conversations.push(value);
      return value;
    });
    const started = vi.fn();
    const received = vi.fn();
    const unhandled = vi.fn();
    h.agent.on("start", started);
    h.agent.on("conversation", received);
    h.agent.on("unhandledError", unhandled);

    await h.agent.start({ retryDelay: 0 });
    callbacks[0]!(new Error("retryable notification callback error"));
    await setImmediate();
    expect(nativeOpen).toHaveBeenCalledOnce();
    expect(h.conversations[0]!.end).not.toHaveBeenCalled();
    expect(h.messages[0]!.end).not.toHaveBeenCalled();
    expect(unhandled).not.toHaveBeenCalled();

    closes[0]!();
    await vi.waitFor(() => expect(nativeOpen).toHaveBeenCalledTimes(2));
    callbacks[1]!(null, {
      id: () => "recovered-group",
      groupMetadata: async () => ({
        conversationType: () => ConversationType.Group,
      }),
    });
    await vi.waitFor(() => expect(received).toHaveBeenCalledOnce());
    expect(received.mock.calls[0]![0].conversation.id).toBe("recovered-group");
    expect(started).toHaveBeenCalledOnce();
    expect(h.stream).toHaveBeenCalledOnce();
    expect(h.streamAllMessages).toHaveBeenCalledOnce();
    expect(unhandled).not.toHaveBeenCalled();

    const terminal = new Error(
      "[LocalDeliveryError::NetworkRecoveryExhausted] budget exhausted",
    );
    callbacks[1]!(terminal);
    await vi.waitFor(() => expect(unhandled).toHaveBeenCalledOnce());
    expect(unhandled.mock.calls[0]![0].cause).toBe(terminal);
    expect(h.conversations[0]!.end).toHaveBeenCalledOnce();
    expect(h.messages[0]!.end).toHaveBeenCalledOnce();
    expect(started).toHaveBeenCalledOnce();
    expect(h.stream).toHaveBeenCalledOnce();
  });

  it("closes both streams after a clean notification end", async () => {
    const h = harness();
    const stopped = vi.fn();
    const unhandled = vi.fn();
    h.agent.on("stop", stopped);
    h.agent.on("unhandledError", unhandled);
    await h.agent.start();

    h.conversations[0]!.options?.onEnd?.();
    await vi.waitFor(() => expect(stopped).toHaveBeenCalledOnce());
    expect(h.conversations[0]!.end).toHaveBeenCalledOnce();
    expect(h.messages[0]!.end).toHaveBeenCalledOnce();
    expect(unhandled).not.toHaveBeenCalled();
  });

  it("reports an immediate native read failure after local readiness and cleanup", async () => {
    const h = harness();
    const closed = Promise.withResolvers<void>();
    const cause = new Error("first native read failed");
    const start = vi.fn();
    const error = vi.fn();
    const nativeClose = vi.fn();
    h.agent.on("start", start);
    h.agent.on("unhandledError", error);
    h.stream.mockImplementationOnce(async (options) => {
      const value = { options, end: vi.fn(() => closed.promise) };
      h.conversations.push(value);
      return value;
    });
    h.streamAllMessages.mockImplementationOnce(async (options) => {
      await Promise.resolve();
      const reader = {
        nextDelivery: vi.fn().mockRejectedValue(cause),
        close: nativeClose,
      } as unknown as MessageReaderSource<DecodedMessage<BuiltInContentTypes>>;
      return new MessageStream(
        reader,
        (message) => message,
        options,
      ) as unknown as Awaited<ReturnType<typeof h.streamAllMessages>>;
    });
    await h.agent.start();
    await vi.waitFor(() =>
      expect(h.conversations[0]!.end).toHaveBeenCalledOnce(),
    );
    expect(start).toHaveBeenCalledOnce();
    expect(nativeClose).toHaveBeenCalledOnce();
    expect(error).not.toHaveBeenCalled();
    await h.agent.start();
    expect(h.stream).toHaveBeenCalledOnce();
    closed.resolve();
    await vi.waitFor(() => expect(error).toHaveBeenCalledOnce());
    expect(error.mock.calls[0]![0].cause).toBe(cause);
    expect(start).toHaveBeenCalledOnce();
    expect(h.streamAllMessages).toHaveBeenCalledOnce();
    await h.agent.stop();
  });

  it("does not report readiness when a stream fails as setup completes", async () => {
    const h = harness();
    const closed = Promise.withResolvers<void>();
    const cause = new Error("terminal error during setup");
    const onStart = vi.fn();
    const onError = vi.fn();
    h.agent.on("start", onStart);
    h.agent.on("unhandledError", onError);
    h.stream.mockImplementationOnce(async (options) => {
      const value = { options, end: vi.fn(() => closed.promise) };
      h.conversations.push(value);
      return value;
    });
    h.streamAllMessages.mockImplementationOnce(async (options) => {
      const failAfter = (remaining: number) => {
        queueMicrotask(() => {
          if (remaining > 0) {
            failAfter(remaining - 1);
          } else {
            options?.onEnd?.();
            void Promise.resolve(options?.onError?.(cause)).catch(
              () => undefined,
            );
          }
        });
      };
      failAfter(3);
      return { options, end: vi.fn(async () => undefined) };
    });

    await h.agent.start();
    await vi.waitFor(() =>
      expect(h.conversations[0]!.end).toHaveBeenCalledOnce(),
    );
    expect(onStart).not.toHaveBeenCalled();
    expect(onError).not.toHaveBeenCalled();
    await h.agent.start();
    expect(h.stream).toHaveBeenCalledOnce();
    closed.resolve();
    await vi.waitFor(() => expect(onError).toHaveBeenCalledOnce());
    expect(onError.mock.calls[0]![0].cause).toBe(cause);
    expect(onStart).not.toHaveBeenCalled();
    await h.agent.stop();
  });

  it("closes a pending open before error middleware acquires a replacement reader", async () => {
    const h = harness();
    const pending =
      Promise.withResolvers<Awaited<ReturnType<typeof h.streamAllMessages>>>();
    const closed = Promise.withResolvers<void>();
    const cause = new Error("conversation stream failed during message open");
    let owner = false;
    const openMessages = h.streamAllMessages.getMockImplementation()!;
    h.streamAllMessages.mockImplementation(async (options) => {
      if (owner) throw new Error("message reader already owned");
      owner = true;
      const stream = await openMessages(options);
      stream.end.mockImplementation(async () => {
        owner = false;
      });
      return stream;
    });
    h.streamAllMessages.mockImplementationOnce(() => {
      owner = true;
      return pending.promise;
    });
    const handler = vi.fn<AgentErrorMiddleware>(
      async (error, _context, next) => {
        expect((error as Error).cause).toBe(cause);
        expect(owner).toBe(false);
        await h.agent.start();
        await next();
      },
    );
    h.agent.errors.use(handler);
    const opening = h.agent.start();
    await vi.waitFor(() => expect(h.streamAllMessages).toHaveBeenCalledOnce());
    const failure = failNotification(h.conversations[0]!.options, cause);
    await setImmediate();
    expect(handler).not.toHaveBeenCalled();
    const old = {
      options: undefined,
      end: vi.fn(async () => {
        await closed.promise;
        owner = false;
      }),
    };
    pending.resolve(old);
    await vi.waitFor(() => expect(old.end).toHaveBeenCalledOnce());
    expect(handler).not.toHaveBeenCalled();
    closed.resolve();
    await Promise.all([opening, failure]);
    await vi.waitFor(() => expect(handler).toHaveBeenCalledOnce());
    expect(h.streamAllMessages).toHaveBeenCalledTimes(2);
    expect(h.messages[0]!.end).not.toHaveBeenCalled();
    await h.agent.stop();
  });

  it("keeps the terminal cause if a pending open also rejects", async () => {
    const h = harness();
    const pending =
      Promise.withResolvers<Awaited<ReturnType<typeof h.streamAllMessages>>>();
    const cause = new Error("conversation stream failed");
    const error = vi.fn();
    h.agent.on("unhandledError", error);
    h.streamAllMessages.mockReturnValueOnce(pending.promise);
    const opening = h.agent.start();
    await vi.waitFor(() => expect(h.streamAllMessages).toHaveBeenCalledOnce());
    const failure = failNotification(h.conversations[0]!.options, cause);
    pending.reject(new Error("message open failed later"));
    await Promise.all([opening, failure]);
    await vi.waitFor(() => expect(error).toHaveBeenCalledOnce());
    expect(error.mock.calls[0]![0].cause).toBe(cause);
    await h.agent.stop();
  });

  it("does not dispatch an old message after its SDK lookup resumes", async () => {
    const h = harness();
    const lookup = Promise.withResolvers<
      Group<BuiltInContentTypes> | undefined
    >();
    const getConversation = vi.mocked(
      h.client.conversations.getConversationById,
    );
    getConversation.mockReturnValueOnce(lookup.promise);
    const message = vi.fn();
    h.agent.on("message", message);
    h.agent.errors.use(async (_error, _context, next) => {
      await h.agent.start();
      await next();
    });
    await h.agent.start();
    const value = {
      content: "late",
      senderInboxId: "peer",
      conversationId: "group",
      contentType: {
        authorityId: "xmtp.org",
        typeId: "text",
        versionMajor: 1,
        versionMinor: 0,
      },
    } as DecodedMessage<BuiltInContentTypes>;
    const delivery = h.messages[0]!.options?.onValue?.(value);
    expect(getConversation).toHaveBeenCalledOnce();
    await Promise.resolve(
      h.messages[0]!.options?.onError?.(new Error("terminal")),
    );
    lookup.resolve({} as Group<BuiltInContentTypes>);
    await delivery;
    expect(message).not.toHaveBeenCalled();
    expect(h.streamAllMessages).toHaveBeenCalledTimes(2);
    await h.agent.stop();
  });

  it("does not renew an exhausted stream when error middleware handles it", async () => {
    const { agent, messages, conversations, stream, streamAllMessages } =
      harness();
    const handle = vi.fn<AgentErrorMiddleware>(
      async (_error, _context, next) => {
        await Promise.resolve();
        await next();
      },
    );
    agent.errors.use(handle);
    await agent.start();
    await Promise.resolve(
      messages[0]!.options?.onError?.(new Error("budget exhausted")),
    );
    expect(handle).toHaveBeenCalledOnce();
    expect(conversations[0]!.end).toHaveBeenCalledOnce();
    expect(messages[0]!.end).toHaveBeenCalledOnce();
    expect(stream).toHaveBeenCalledOnce();
    expect(streamAllMessages).toHaveBeenCalledOnce();
    await agent.start();
    expect(streamAllMessages).toHaveBeenCalledTimes(2);
    await agent.stop();
  });

  it.each(["messages", "conversations"] as const)(
    "can start inside the error event after %s exhaust, twice on the same client",
    async (failed) => {
      const h = harness();
      const start = vi.fn();
      h.agent.on("start", start);
      h.agent.on("unhandledError", () => {
        void h.agent.start();
      });
      await h.agent.start();
      for (let i = 0; i < 2; i++) {
        const cause = new Error("budget exhausted");
        await (failed === "conversations"
          ? failNotification(h.conversations[i]!.options, cause)
          : Promise.resolve(h.messages[i]!.options?.onError?.(cause)));
        await vi.waitFor(() => expect(start).toHaveBeenCalledTimes(i + 2));
        expect(h.conversations[i]!.end).toHaveBeenCalledOnce();
        expect(h.messages[i]!.end).toHaveBeenCalledOnce();
        // A delayed old error cannot stop or reopen the replacement.
        const late = new Error("late old failure");
        await (failed === "conversations"
          ? failNotification(h.conversations[i]!.options, late)
          : Promise.resolve(h.messages[i]!.options?.onError?.(late)));
        expect(h.messages[i + 1]!.end).not.toHaveBeenCalled();
        expect(h.streamAllMessages).toHaveBeenCalledTimes(i + 2);
      }
      expect(h.agent.client).toBe(h.client);
      await h.agent.stop();
    },
  );

  it("closes both streams if one close rejects and permits a fresh start", async () => {
    const h = harness();
    const error = new Error("budget exhausted");
    const onError = vi.fn();
    h.agent.on("unhandledError", onError);
    await h.agent.start();
    h.conversations[0]!.end.mockRejectedValue(new Error("close failed"));
    await Promise.resolve(h.messages[0]!.options?.onError?.(error));
    expect(h.messages[0]!.end).toHaveBeenCalledOnce();
    expect(onError).toHaveBeenCalledOnce();
    await h.agent.start();
    expect(h.streamAllMessages).toHaveBeenCalledTimes(2);
    await h.agent.stop();
  });

  it.each(["messages", "conversations"] as const)(
    "closes both streams before async error middleware handles a %s failure",
    async (failed) => {
      const h = harness();
      const cause = new Error("Core recovery exhausted");
      const conversationsClosed = Promise.withResolvers<void>();
      const messagesClosed = Promise.withResolvers<void>();
      const handler = vi.fn<AgentErrorMiddleware>(
        async (error, _context, next) => {
          expect(error).toBeInstanceOf(AgentStreamingError);
          expect((error as Error).cause).toBe(cause);
          expect(h.conversations[0]!.end).toHaveBeenCalledOnce();
          expect(h.messages[0]!.end).toHaveBeenCalledOnce();
          await h.agent.start();
          await next();
        },
      );
      h.agent.errors.use(handler);
      await h.agent.start();
      h.conversations[0]!.end.mockReturnValue(conversationsClosed.promise);
      h.messages[0]!.end.mockReturnValue(messagesClosed.promise);
      const failure =
        failed === "conversations"
          ? failNotification(h.conversations[0]!.options, cause)
          : Promise.resolve(h.messages[0]!.options?.onError?.(cause));
      await vi.waitFor(() => expect(h.messages[0]!.end).toHaveBeenCalledOnce());
      expect(handler).not.toHaveBeenCalled();
      conversationsClosed.resolve();
      await Promise.resolve();
      expect(handler).not.toHaveBeenCalled();
      messagesClosed.resolve();
      await failure;
      await vi.waitFor(() => expect(handler).toHaveBeenCalledOnce());
      expect(h.streamAllMessages).toHaveBeenCalledTimes(2);
      expect(h.messages[1]!.end).not.toHaveBeenCalled();
      await h.agent.stop();
    },
  );

  it("closes a partial setup before error middleware explicitly starts again", async () => {
    const h = harness();
    const cause = new Error("message stream setup failed");
    h.streamAllMessages.mockRejectedValueOnce(cause);
    const handler = vi.fn<AgentErrorMiddleware>(
      async (error, _context, next) => {
        expect(error).toBeInstanceOf(AgentStreamingError);
        expect((error as Error).cause).toBe(cause);
        expect(h.conversations[0]!.end).toHaveBeenCalledOnce();
        await h.agent.start();
        await next();
      },
    );
    h.agent.errors.use(handler);
    await h.agent.start();
    expect(handler).toHaveBeenCalledOnce();
    expect(h.stream).toHaveBeenCalledTimes(2);
    expect(h.streamAllMessages).toHaveBeenCalledTimes(2);
    expect(h.conversations[1]!.end).not.toHaveBeenCalled();
    expect(h.messages[0]!.end).not.toHaveBeenCalled();
    await h.agent.stop();
  });

  it("waits for existing cleanup when stop calls overlap", async () => {
    const h = harness();
    await h.agent.start();
    const closed = Promise.withResolvers<void>();
    h.conversations[0]!.end.mockReturnValue(closed.promise);
    const firstStop = h.agent.stop();
    const restart = h.agent.stop().then(() => h.agent.start());
    await setImmediate();
    expect(h.stream).toHaveBeenCalledOnce();
    expect(h.messages[0]!.end).toHaveBeenCalledOnce();
    closed.resolve();
    await Promise.all([firstStop, restart]);
    expect(h.stream).toHaveBeenCalledTimes(2);
    expect(h.conversations[0]!.end).toHaveBeenCalledOnce();
    expect(h.messages[1]!.end).not.toHaveBeenCalled();
    await h.agent.stop();
  });

  it("waits for a late opening to close before a new generation starts", async () => {
    const h = harness();
    const delayed =
      Promise.withResolvers<Awaited<ReturnType<typeof h.stream>>>();
    h.stream.mockReturnValueOnce(delayed.promise);
    const opening = h.agent.start();
    const stopping = h.agent.stop();
    await h.agent.start();
    expect(h.stream).toHaveBeenCalledOnce();
    const old = {
      options: undefined,
      end: vi.fn<() => Promise<void>>(async () => undefined),
    };
    delayed.resolve(old);
    await Promise.all([opening, stopping]);
    await h.agent.start();
    expect(old.end).toHaveBeenCalledOnce();
    expect(h.conversations[0]!.end).not.toHaveBeenCalled();
    expect(h.messages[0]!.end).not.toHaveBeenCalled();
    expect(h.streamAllMessages).toHaveBeenCalledOnce();
    await h.agent.stop();
  });
});
