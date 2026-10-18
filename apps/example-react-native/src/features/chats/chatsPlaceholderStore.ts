import { type Conversation, Group } from "@xmtp/react-native-sdk";

import { errorMessage } from "@/core/sessionError.ts";
import { Store } from "@/core/store.ts";
import {
  conversationInitials,
  conversationTitle,
  type TitleSource,
} from "@/core/titles.ts";
import type { ActiveSession } from "@/session";

export interface ChatRow {
  readonly id: string;
  readonly title: string;
  readonly initials: string;
}

export interface ChatsPlaceholderState {
  readonly loading: boolean;
  readonly rows: readonly ChatRow[];
  readonly error?: string;
}

async function titleSource(conversation: Conversation): Promise<TitleSource> {
  if (conversation instanceof Group) {
    const state = await conversation.state();
    return { id: conversation.id, kind: "group", name: state.name };
  }
  const peer = await conversation.peerInboxId();
  return {
    id: conversation.id,
    kind: "dm",
    ...(peer === null ? {} : { peerInboxId: peer }),
  };
}

/**
 * The placeholder Chats list: allowed conversations by last activity, with
 * their titles. It reloads when the session's streams deliver a conversation,
 * a message, a consent change or a handle change. The chats lane replaces it
 * with `ConversationsStore`.
 */
export class ChatsPlaceholderStore extends Store<ChatsPlaceholderState> {
  readonly #session: ActiveSession;
  readonly #stops: (() => void)[];
  #disposed = false;
  #running = false;
  #again = false;

  constructor(session: ActiveSession) {
    super({ loading: true, rows: [] });
    this.#session = session;
    this.#stops = [
      session.streams.onConversation(() => {
        this.reload();
      }),
      session.streams.onMessage(() => {
        this.reload();
      }),
      session.streams.onEvent((event) => {
        if (event.kind === "changed" || event.kind === "handleChanged") {
          this.reload();
        }
      }),
    ];
    this.reload();
  }

  /** Loads the list; calls during a load run one more load after it. */
  reload(): void {
    if (this.#running) {
      this.#again = true;
      return;
    }
    this.#running = true;
    void this.#load().finally(() => {
      this.#running = false;
      if (this.#again && !this.#disposed) {
        this.#again = false;
        this.reload();
      }
    });
  }

  dispose(): void {
    this.#disposed = true;
    for (const stop of this.#stops) stop();
  }

  #admits(generation: number): boolean {
    return !this.#disposed && this.#session.isCurrent(generation);
  }

  async #load(): Promise<void> {
    const session = this.#session;
    const generation = session.generation;
    try {
      const conversations = await session.client.conversations.list({
        consentStates: ["allowed"],
        orderBy: "lastActivity",
      });
      const sources = await Promise.all(conversations.map(titleSource));
      const peers = sources.flatMap((source) =>
        source.peerInboxId === undefined ? [] : [source.peerInboxId],
      );
      if (peers.length > 0) await session.identities.load(peers);
      if (!this.#admits(generation)) return;
      const rows = sources.map((source): ChatRow => {
        const handle =
          source.peerInboxId === undefined
            ? undefined
            : session.identities.identity(source.peerInboxId)?.handle;
        const named =
          handle === undefined ? source : { ...source, peerHandle: handle };
        const title = conversationTitle(named);
        return {
          id: source.id,
          title,
          initials: conversationInitials(named, title),
        };
      });
      this.setState({ loading: false, rows });
    } catch (error) {
      if (!this.#admits(generation)) return;
      this.setState((state) => ({
        ...state,
        loading: false,
        error: errorMessage(error),
      }));
    }
  }
}
