// The chats an unassigned draft can be assigned to on the recovery screen:
// the Chats list's first page, read with the same query as Chats (allowed
// conversations, latest activity first).

import { CONVERSATION_PAGE_SIZE } from "@/core/requests.ts";
import { Store } from "@/core/store.ts";
import { conversationTitle } from "@/core/titles.ts";
import { chatServices } from "@/features/chat/chatServices";
import { conversationQueries } from "@/features/chats/conversationServices";
import type { ActiveSession } from "@/session";

export interface ChatChoice {
  readonly id: string;
  readonly title: string;
}

export interface RecoveryChatsState {
  readonly chats: readonly ChatChoice[];
}

export class RecoveryChatsStore extends Store<RecoveryChatsState> {
  readonly #session: ActiveSession;
  readonly #mounted: () => boolean;
  #loading = false;

  constructor(session: ActiveSession, mounted: () => boolean) {
    super({ chats: [] });
    this.#session = session;
    this.#mounted = mounted;
  }

  /** Loads the choices once; a failure leaves the list empty. */
  async load(): Promise<void> {
    if (this.#loading || this.getState().chats.length > 0) return;
    this.#loading = true;
    const session = this.#session;
    const generation = session.generation;
    try {
      const { queries } = chatServices(session);
      const conversations = await conversationQueries(session).list(
        "allowed",
        CONVERSATION_PAGE_SIZE,
      );
      const chats = await Promise.all(
        conversations.map(async ({ id }) => ({
          id,
          title: conversationTitle(
            await queries.conversation(id),
            session.preferences.alias(id),
          ),
        })),
      );
      if (this.#mounted() && session.isCurrent(generation)) {
        this.setState({ chats });
      }
    } catch (error) {
      console.warn("Cannot list chats for attachment recovery", error);
    } finally {
      this.#loading = false;
    }
  }
}
