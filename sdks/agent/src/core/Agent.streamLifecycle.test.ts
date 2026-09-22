import type { Client } from "@xmtp/node-sdk";
import { describe, expect, it, vi } from "vitest";

import { Agent } from "@/core/Agent";

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
    const value = { options, end: vi.fn(async () => undefined) };
    conversations.push(value);
    return value;
  });
  const streamAllMessages = vi.fn(async (options: MessageOptions) => {
    const value = { options, end: vi.fn(async () => undefined) };
    messages.push(value);
    return value;
  });
  const client = {
    conversations: { stream, streamAllMessages },
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

  it("does not renew an exhausted stream when error middleware handles it", async () => {
    const { agent, messages, conversations, stream, streamAllMessages } =
      harness();
    const handle = vi.fn();
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
        await Promise.resolve(
          h[failed][i]!.options?.onError?.(new Error("budget exhausted")),
        );
        await vi.waitFor(() => expect(start).toHaveBeenCalledTimes(i + 2));
        expect(h.conversations[i]!.end).toHaveBeenCalledOnce();
        expect(h.messages[i]!.end).toHaveBeenCalledOnce();
        // A delayed old error cannot stop or reopen the replacement.
        await Promise.resolve(
          h[failed][i]!.options?.onError?.(new Error("late old failure")),
        );
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

  it("discards a late opening after stop without closing a newer generation", async () => {
    const h = harness();
    const delayed =
      Promise.withResolvers<Awaited<ReturnType<typeof h.stream>>>();
    h.stream.mockReturnValueOnce(delayed.promise);
    const opening = h.agent.start();
    await h.agent.stop();
    await h.agent.start();
    const old = { options: undefined, end: vi.fn(async () => undefined) };
    delayed.resolve(old);
    await opening;
    expect(old.end).toHaveBeenCalledOnce();
    expect(h.conversations[0]!.end).not.toHaveBeenCalled();
    expect(h.messages[0]!.end).not.toHaveBeenCalled();
    expect(h.streamAllMessages).toHaveBeenCalledOnce();
    await h.agent.stop();
  });
});
