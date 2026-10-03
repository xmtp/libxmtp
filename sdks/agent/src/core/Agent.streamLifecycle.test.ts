import { setImmediate } from "node:timers/promises";

import { ReaderStream } from "@node-private/streams/reader.js";
import {
  ConversationStream,
  MessageStream,
  Group,
  type Client,
  type Conversation,
  type Message,
  type StreamOptions,
} from "@xmtp/node-sdk";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Agent } from "./Agent";
import { AgentStreamingError } from "./AgentError";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
function reader<T>(options?: StreamOptions) {
  let deliver!: (value: T) => void | Promise<void>;
  return {
    options,
    ready: vi.fn<() => Promise<void>>(async () => undefined),
    end: vi.fn<() => Promise<void>>(async () => undefined),
    onValue: vi.fn((callback: typeof deliver) => {
      deliver = callback;
      return new Promise<void>(() => {});
    }),
    value: (value: T) => deliver(value),
    close: (reason: Parameters<NonNullable<StreamOptions["onClose"]>>[0]) =>
      options?.onClose?.(reason),
  };
}
function harness() {
  const conversations: ReturnType<typeof reader<Conversation>>[] = [];
  const messages: ReturnType<typeof reader<Message>>[] = [];
  const openConversation = vi
    .spyOn(ConversationStream, "open")
    .mockImplementation((_client, _selection, options) => {
      const stream = reader<Conversation>(options);
      conversations.push(stream);
      return stream as unknown as ConversationStream;
    });
  const openMessage = vi
    .spyOn(MessageStream, "open")
    .mockImplementation((_client, _selection, options) => {
      const stream = reader<Message>(options);
      messages.push(stream);
      return stream as unknown as MessageStream;
    });
  const group = Object.defineProperty(
    Object.create(Group.prototype) as Group,
    "id",
    { value: "group" },
  );
  const getById = vi.fn(async () => group as Conversation | undefined);
  const client = {
    inboxId: "agent",
    conversations: { getById },
  } as unknown as Client;
  const agent = new Agent({ client });
  return {
    agent,
    client,
    group,
    conversations,
    messages,
    openConversation,
    openMessage,
    getById,
  };
}
const message = {
  id: "message",
  senderInboxId: "sender",
  conversationId: "group",
  content: { kind: "text", value: "hello" },
} as unknown as Message;
afterEach(() => vi.restoreAllMocks());

describe("Agent stream lifecycle", () => {
  it.each([
    ["abort", false],
    ["end", false],
    ["abort", true],
    ["end", true],
  ] as const)(
    "cleans up after a throwing close callback on a real reader %s (reentry: %s) and restarts",
    async (mode, reentry) => {
      const streams: ReaderStream<unknown>[] = [];
      const rawReaders: { end: ReturnType<typeof vi.fn> }[] = [];
      const makeStream = <T>(owner: object, options?: StreamOptions) => {
        const pending = deferred<T | undefined>();
        const raw = {
          next: () => pending.promise,
          end: vi.fn(async () => pending.resolve(undefined)),
        };
        rawReaders.push(raw);
        const stream = new ReaderStream<T>(async () => raw, owner, {
          signal: options?.signal,
          onClose: options?.onClose,
        });
        streams.push(stream);
        return stream;
      };
      const conversations = vi
        .spyOn(ConversationStream, "open")
        .mockImplementation(
          (owner, _selection, options) =>
            makeStream<Conversation>(owner, options) as ConversationStream,
        );
      const messages = vi
        .spyOn(MessageStream, "open")
        .mockImplementation(
          (owner, _selection, options) =>
            makeStream<Message>(owner, options) as MessageStream,
        );
      const client = { inboxId: "agent" } as Client;
      const agent = new Agent({ client });
      const abort = new AbortController();
      const cause = new Error("app close failed");
      let restart: Promise<void> | undefined;
      const closed = vi.fn(() => {
        if (reentry && !restart) {
          restart = agent.stop().then(() => agent.start());
        }
        throw cause;
      });
      const stopped = vi.fn();
      const reported = vi.spyOn(console, "error").mockImplementation(() => {});
      agent.on("stop", stopped);
      try {
        await agent.start({ signal: abort.signal, onClose: closed });
        if (mode === "abort") abort.abort();
        else await streams[0]!.end();
        await vi.waitFor(() => expect(stopped).toHaveBeenCalledOnce());
        expect(rawReaders[0]!.end).toHaveBeenCalledOnce();
        expect(rawReaders[1]!.end).toHaveBeenCalledOnce();
        expect(reported).toHaveBeenCalledWith(
          "XMTP stream close callback failed",
          cause,
        );
        if (restart) await restart;
        else await agent.start();
        expect(conversations).toHaveBeenCalledTimes(2);
        expect(messages).toHaveBeenCalledTimes(2);
        // A late close from the old generation cannot stop the replacements.
        await streams[0]!.end();
        expect(rawReaders[2]!.end).not.toHaveBeenCalled();
        expect(rawReaders[3]!.end).not.toHaveBeenCalled();
      } finally {
        await agent.stop();
        await Promise.allSettled(streams.map((stream) => stream.end()));
      }
    },
  );
  it("opens both readers once and waits for local readiness", async () => {
    const h = harness();
    const start = vi.fn();
    h.agent.on("start", start);
    await h.agent.start();
    await h.agent.start();
    expect(h.openConversation).toHaveBeenCalledOnce();
    expect(h.openMessage).toHaveBeenCalledOnce();
    expect(h.conversations[0]!.ready).toHaveBeenCalledOnce();
    expect(h.messages[0]!.ready).toHaveBeenCalledOnce();
    expect(start).toHaveBeenCalledOnce();
    await h.agent.stop();
  });
  it("closes both readers after a clean end", async () => {
    const h = harness();
    const closed = vi.fn();
    await h.agent.start({ onClose: closed });
    h.conversations[0]!.close({ kind: "closed" });
    await vi.waitFor(() => expect(h.messages[0]!.end).toHaveBeenCalledOnce());
    expect(h.conversations[0]!.end).toHaveBeenCalledOnce();
    expect(closed).toHaveBeenCalledWith({ kind: "closed" });
  });
  it("does not emit a group after a conversation handler stops the agent", async () => {
    const h = harness();
    const group = vi.fn();
    h.agent.on("group", group);
    h.agent.on("conversation", () => {
      void h.agent.stop();
    });
    await h.agent.start();
    await h.conversations[0]!.value(h.group);
    expect(group).not.toHaveBeenCalled();
  });
  it("does not dispatch an old message after its conversation lookup resumes", async () => {
    const h = harness();
    const lookup = deferred<Conversation | undefined>();
    h.getById.mockReturnValueOnce(lookup.promise);
    const received = vi.fn();
    h.agent.on("message", received);
    await h.agent.start();
    const delivery = h.messages[0]!.value(message);
    await h.agent.stop();
    await h.agent.start();
    lookup.resolve(h.group);
    await delivery;
    expect(received).not.toHaveBeenCalled();
    await h.agent.stop();
  });
  it("does not continue old middleware after a replacement starts", async () => {
    const h = harness();
    const gate = deferred<void>();
    const received = vi.fn();
    h.agent.on("message", received);
    h.agent.use(async (_ctx, next) => {
      await gate.promise;
      await next();
    });
    await h.agent.start();
    const delivery = h.messages[0]!.value(message);
    await setImmediate();
    await h.agent.stop();
    await h.agent.start();
    gate.resolve();
    await delivery;
    expect(received).not.toHaveBeenCalled();
    await h.agent.stop();
  });
  it("does not emit a generic message after the topic handler stops the agent", async () => {
    const h = harness();
    const received = vi.fn();
    h.agent.on("message", received);
    h.agent.on("text", () => {
      void h.agent.stop();
    });
    await h.agent.start();
    await h.messages[0]!.value(message);
    expect(received).not.toHaveBeenCalled();
  });
  it.each(["unknown", "custom"] as const)(
    "routes %s content through middleware and unknownMessage",
    async (kind) => {
      const h = harness();
      const middleware = vi.fn(async (_ctx, next: () => Promise<void> | void) =>
        next(),
      );
      const unknown = vi.fn();
      const received = vi.fn();
      h.agent.use(middleware);
      h.agent.on("unknownMessage", unknown);
      h.agent.on("message", received);
      await h.agent.start();
      const undecoded = Object.assign({}, message, {
        content: { kind },
      }) as Message;
      await h.messages[0]!.value(undecoded);
      expect(middleware).toHaveBeenCalledOnce();
      expect(unknown).toHaveBeenCalledOnce();
      expect(received).toHaveBeenCalledOnce();
      expect(h.messages[0]!.end).not.toHaveBeenCalled();
      await h.agent.stop();
    },
  );
  it("routes a failed message lookup through error middleware and keeps reading", async () => {
    const h = harness();
    const cause = new Error("lookup failed");
    h.getById.mockRejectedValueOnce(cause);
    const onError = vi.fn((_error, _ctx, next: () => void) => next());
    const received = vi.fn();
    h.agent.errors.use(onError);
    h.agent.on("message", received);
    await h.agent.start();
    await h.messages[0]!.value(message);
    expect(onError).toHaveBeenCalledWith(
      cause,
      expect.objectContaining({ client: h.client, message }),
      expect.any(Function),
    );
    expect(h.messages[0]!.end).not.toHaveBeenCalled();
    expect(h.conversations[0]!.end).not.toHaveBeenCalled();
    await h.messages[0]!.value(message);
    expect(received).toHaveBeenCalledOnce();
    await h.agent.stop();
  });
  it("rejects an unhandled value so the reader cannot acknowledge it", async () => {
    const h = harness();
    const cause = new Error("lookup failed");
    h.getById.mockRejectedValueOnce(cause);
    const unhandled = vi.fn();
    h.agent.on("unhandledError", unhandled);
    await h.agent.start();
    await expect(h.messages[0]!.value(message)).rejects.toThrow(
      "Agent value processing failed",
    );
    expect(unhandled).toHaveBeenCalledOnce();
    expect(unhandled).toHaveBeenCalledWith(cause);
    await h.agent.stop();
  });
  it.each(["unhandled", "stopped"] as const)(
    "does not make an acknowledging read after a %s value",
    async (disposition) => {
      const cause = new Error("lookup failed");
      const conversationRead = deferred<Conversation | undefined>();
      const conversationEnd = vi.fn(async () =>
        conversationRead.resolve(undefined),
      );
      const messageNext = vi
        .fn<() => Promise<Message | undefined>>()
        .mockResolvedValueOnce(message)
        .mockResolvedValue(undefined);
      const messageEnd = vi.fn(async () => undefined);
      vi.spyOn(ConversationStream, "open").mockImplementation(
        (owner, _selection, options) =>
          new ReaderStream<Conversation>(
            async () => ({
              next: () => conversationRead.promise,
              end: conversationEnd,
            }),
            owner,
            options && { signal: options.signal, onClose: options.onClose },
          ) as ConversationStream,
      );
      vi.spyOn(MessageStream, "open").mockImplementation(
        (owner, _selection, options) =>
          new ReaderStream<Message>(
            async () => ({ next: messageNext, end: messageEnd }),
            owner,
            options && { signal: options.signal, onClose: options.onClose },
          ) as MessageStream,
      );
      const client = {
        inboxId: "agent",
        conversations: { getById: vi.fn().mockRejectedValue(cause) },
      } as unknown as Client;
      const agent = new Agent({ client });
      const unhandled = vi.fn();
      const closed = vi.fn();
      agent.on("unhandledError", unhandled);
      if (disposition === "stopped") agent.errors.use(() => undefined);
      await agent.start({ onClose: closed });
      await vi.waitFor(() => expect(messageEnd).toHaveBeenCalledOnce());
      expect(closed).toHaveBeenCalledWith({
        kind: "failed",
        error: expect.objectContaining({ cause }),
      });
      expect(messageNext).toHaveBeenCalledOnce();
      expect(conversationEnd).toHaveBeenCalledOnce();
      if (disposition === "unhandled") {
        expect(unhandled).toHaveBeenCalledOnce();
        expect(unhandled).toHaveBeenCalledWith(cause);
      } else {
        expect(unhandled).not.toHaveBeenCalled();
      }
      await agent.stop();
    },
  );
  it.each(["middleware", "message listener", "conversation listener"] as const)(
    "rejects a %s error when custom error middleware stops",
    async (origin) => {
      const h = harness();
      const cause = new Error("app failed");
      const onError = vi.fn(() => undefined);
      const unhandled = vi.fn();
      h.agent.errors.use(onError);
      h.agent.on("unhandledError", unhandled);
      if (origin === "middleware")
        h.agent.use(() => {
          throw cause;
        });
      if (origin === "message listener")
        h.agent.on("message", () => {
          throw cause;
        });
      if (origin === "conversation listener")
        h.agent.on("conversation", () => {
          throw cause;
        });
      await h.agent.start();
      await expect(
        origin === "conversation listener"
          ? h.conversations[0]!.value(h.group)
          : h.messages[0]!.value(message),
      ).rejects.toThrow("Agent value processing failed");
      expect(onError).toHaveBeenCalledOnce();
      expect(unhandled).not.toHaveBeenCalled();
      await h.agent.stop();
    },
  );
  it("routes a throwing conversation listener through error middleware", async () => {
    const h = harness();
    const cause = new Error("listener failed");
    const onError = vi.fn((_error, _ctx, next: () => void) => next());
    h.agent.errors.use(onError);
    h.agent.on("conversation", () => {
      throw cause;
    });
    await h.agent.start();
    await h.conversations[0]!.value(h.group);
    expect(onError).toHaveBeenCalledWith(
      cause,
      expect.objectContaining({ client: h.client, conversation: h.group }),
      expect.any(Function),
    );
    expect(h.conversations[0]!.end).not.toHaveBeenCalled();
    expect(h.messages[0]!.end).not.toHaveBeenCalled();
    await h.messages[0]!.value(message);
    await h.agent.stop();
  });
  it("keeps an exhausted stream ended when error middleware handles it", async () => {
    const h = harness();
    const cause = new Error("budget exhausted");
    const handled = vi.fn();
    h.agent.errors.use((error, _ctx, next) => {
      handled(error);
      void next();
    });
    await h.agent.start();
    h.messages[0]!.close({ kind: "failed", error: cause });
    await vi.waitFor(() => expect(handled).toHaveBeenCalledOnce());
    expect(handled.mock.calls[0]![0]).toBeInstanceOf(AgentStreamingError);
    expect(h.openMessage).toHaveBeenCalledOnce();
    expect(h.messages[0]!.end).toHaveBeenCalledOnce();
  });
  it("closes a partial setup before error middleware starts a replacement", async () => {
    const h = harness();
    h.openMessage.mockImplementationOnce(() => {
      throw new Error("open failed");
    });
    h.agent.errors.use(async () => {
      expect(h.conversations[0]!.end).toHaveBeenCalledOnce();
      await h.agent.start();
    });
    await h.agent.start();
    expect(h.openConversation).toHaveBeenCalledTimes(2);
    await h.agent.stop();
  });
  it("closes both readers if one close rejects and permits a fresh start", async () => {
    const h = harness();
    await h.agent.start();
    h.conversations[0]!.end.mockRejectedValueOnce(new Error("close failed"));
    await expect(h.agent.stop()).rejects.toThrow("close failed");
    expect(h.messages[0]!.end).toHaveBeenCalledOnce();
    await h.agent.start();
    expect(h.openMessage).toHaveBeenCalledTimes(2);
    await h.agent.stop();
  });
  it("waits for cleanup when stop calls overlap", async () => {
    const h = harness();
    const gate = deferred<void>();
    await h.agent.start();
    h.messages[0]!.end.mockReturnValueOnce(gate.promise);
    const first = h.agent.stop();
    const second = h.agent.stop();
    const finished = vi.fn();
    void second.then(finished);
    await setImmediate();
    expect(finished).not.toHaveBeenCalled();
    gate.resolve();
    await Promise.all([first, second]);
    expect(h.messages[0]!.end).toHaveBeenCalledOnce();
  });
  it("fences startup after stop while local readiness is pending", async () => {
    const h = harness();
    const gate = deferred<void>();
    h.openConversation.mockImplementationOnce(
      (_client, _selection, options) => {
        const stream = reader<Conversation>(options);
        stream.ready.mockReturnValueOnce(gate.promise);
        h.conversations.push(stream);
        return stream as unknown as ConversationStream;
      },
    );
    const started = vi.fn();
    h.agent.on("start", started);
    const opening = h.agent.start();
    await h.agent.stop();
    gate.resolve();
    await opening;
    expect(started).not.toHaveBeenCalled();
    expect(h.openMessage).not.toHaveBeenCalled();
    expect(h.conversations[0]!.end).toHaveBeenCalledOnce();
  });
  it("ignores close notifications from an old generation", async () => {
    const h = harness();
    const errors = vi.fn();
    h.agent.on("unhandledError", errors);
    await h.agent.start();
    await h.agent.stop();
    await h.agent.start();
    h.messages[0]!.close({ kind: "failed", error: new Error("old") });
    await setImmediate();
    expect(errors).not.toHaveBeenCalled();
    expect(h.messages[1]!.end).not.toHaveBeenCalled();
    await h.agent.stop();
  });
});
