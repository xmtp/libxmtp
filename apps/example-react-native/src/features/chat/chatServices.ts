// Per-session chat services (App/Features/Chat/ChatServices.swift): the SDK
// queries, the transcript cache, the send journal and its coordinator. Every
// chat screen, the Settings "Send needs review" section and the sheets of one
// session share them; a new session gets new ones.

import { TranscriptCache } from "@/core/chatHistory.ts";
import type { ChatQueries } from "@/core/chatHistory.ts";
import { SendCoordinator, type SendJournal } from "@/core/sendJournal.ts";
import { SdkChatQueries } from "@/features/chat/sdkChatQueries";
import { randomUuid } from "@/platform/random";
import type { ActiveSession } from "@/session";

export interface ChatServices {
  readonly session: ActiveSession;
  readonly queries: ChatQueries;
  readonly cache: TranscriptCache;
  readonly journal: SendJournal;
  readonly coordinator: SendCoordinator;
  /** A new journal entry ID (an uppercase UUID, like iOS). */
  readonly newId: () => string;
  readonly now: () => number;
  /** Runs work as session work: sign out waits for it before ending the client. */
  readonly run: (work: () => Promise<void>) => void;
}

const services = new WeakMap<ActiveSession, ChatServices>();

/** The chat services of `session`, created on first use. */
export function chatServices(session: ActiveSession): ChatServices {
  const existing = services.get(session);
  if (existing !== undefined) return existing;
  const queries = new SdkChatQueries(session.client, session.identities);
  const journal = session.sendJournal();
  const coordinator = new SendCoordinator(
    journal,
    {
      consent: async (id) => (await queries.conversation(id)).consent,
      message: async (id) => {
        const message = await queries.message(id);
        return message === undefined
          ? undefined
          : {
              conversationId: message.conversationId,
              published: message.delivery === "published",
            };
      },
      publish: (conversationId, messageId) =>
        queries.publish(conversationId, messageId),
    },
    () => session.isCurrent(),
  );
  const cache = new TranscriptCache();
  // Every deletion marks the cache, also for chats that are closed, so a
  // reopened chat never shows a deleted body (AppGraph `projectEvent`).
  const stopDeletions = session.streams.onEvent((event) => {
    if (event.kind !== "messageDeleted" || !session.isCurrent()) return;
    cache.markDeleted(event.conversationId, event.messageId);
  });
  session.scope.addCleanup(stopDeletions);
  session.scope.addFinalCleanup(() => {
    cache.clear();
  });
  const value: ChatServices = {
    session,
    queries,
    cache,
    journal,
    coordinator,
    newId: randomUuid,
    now: () => Date.now(),
    run: (work) => {
      void session.scope.add(() => work());
    },
  };
  services.set(session, value);
  return value;
}
