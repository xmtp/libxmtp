// The chats an unassigned draft can be assigned to on the recovery screen.
// iOS lists `graph.conversations.state.rows`: the Chats list's first page,
// the 50 allowed conversations with the latest activity first.

import { Store } from "@/core/store.ts";
import { conversationTitle } from "@/core/titles.ts";
import { chatServices } from "@/features/chat/chatServices";
import type { ActiveSession } from "@/session";

/** The Chats list's first page (`ConversationsStore.limit`). */
const CHATS_PAGE = 50;

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

  /** Loads the choices once; a failure leaves the list empty, like iOS. */
  async load(): Promise<void> {
    if (this.#loading || this.getState().chats.length > 0) return;
    this.#loading = true;
    const session = this.#session;
    const generation = session.generation;
    try {
      const { queries } = chatServices(session);
      const conversations = await session.client.conversations.list({
        consentStates: ["allowed"],
        limit: CHATS_PAGE,
        orderBy: "lastActivity",
      });
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
