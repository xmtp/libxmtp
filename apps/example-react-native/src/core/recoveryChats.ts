// The chats an unassigned draft can be assigned to on the recovery screen:
// the Chats list's first page, read with the same query as Chats (allowed
// conversations, latest activity first).

import { type ConversationQueries, LOAD_FAILED } from "./conversationList.ts";
import { CONVERSATION_PAGE_SIZE } from "./requests.ts";
import { Store } from "./store.ts";
import { type TitleSource, conversationTitle } from "./titles.ts";

export interface ChatChoice {
  readonly id: string;
  readonly title: string;
}

export interface RecoveryChatsState {
  readonly chats: readonly ChatChoice[];
  readonly error?: string;
}

export interface RecoveryChatsPorts {
  /** The Chats list query. */
  readonly queries: Pick<ConversationQueries, "list">;
  /** The conversation's title source: group name, or DM peer and handle. */
  conversation(id: string): Promise<TitleSource>;
  /** The local name of a conversation, if any. */
  alias(id: string): string | undefined;
  /** False once the screen closed or its session ended. */
  admit(): boolean;
}

export class RecoveryChatsStore extends Store<RecoveryChatsState> {
  readonly #ports: RecoveryChatsPorts;
  #loading = false;

  constructor(ports: RecoveryChatsPorts) {
    super({ chats: [] });
    this.#ports = ports;
  }

  /**
   * Loads the choices, unless a load runs or some loaded. A failure shows
   * `LOAD_FAILED`; the next load (Retry) clears it while it runs.
   */
  async load(): Promise<void> {
    if (this.#loading || this.getState().chats.length > 0) return;
    this.#loading = true;
    this.setState((state) =>
      state.error === undefined ? state : { chats: state.chats },
    );
    try {
      const records = await this.#ports.queries.list(
        "allowed",
        CONVERSATION_PAGE_SIZE,
      );
      const chats = await Promise.all(
        records.map(async ({ id }) => ({
          id,
          title: conversationTitle(
            await this.#ports.conversation(id),
            this.#ports.alias(id),
          ),
        })),
      );
      if (this.#ports.admit()) this.setState({ chats });
    } catch (error) {
      console.warn("Cannot list chats for attachment recovery", error);
      if (this.#ports.admit()) this.setState({ chats: [], error: LOAD_FAILED });
    } finally {
      this.#loading = false;
    }
  }
}
