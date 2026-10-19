// Media and files of one conversation
// (ExampleCore/Attachments/SharedMediaStore.swift): the 50 newest file
// messages and the conversation's consent, split into "Photos and videos"
// and "Files" by file name.

import { isVisualFile } from "./attachments.ts";
import type { ChatMessage, ConversationRecord } from "./chatMessages.ts";
import { Store } from "./store.ts";

export const SharedMediaCopy = {
  title: "Media and files",
  intro: "Download a file to verify it before you open or save it.",
  photos: "Photos and videos",
  files: "Files",
  viewInChat: "View in chat",
  limit: "Only the latest 50 file messages load here.",
  loadFailed: "Cannot load media and files. Try again.",
  anchorFailed: "Cannot open this position. Try again.",
} as const;

export interface SharedMediaQueries {
  conversation(id: string): Promise<ConversationRecord>;
  /** The 50 newest remote-attachment messages, newest first. */
  media(conversationId: string): Promise<readonly ChatMessage[]>;
}

export interface SharedMediaState {
  readonly rows: readonly ChatMessage[];
  /** Consent is allowed: downloads are possible. */
  readonly allowed: boolean;
  readonly loading: boolean;
  readonly error?: string;
}

/** True for a file message under "Photos and videos". */
function isVisualRow(row: ChatMessage): boolean {
  return (
    row.content.kind === "attachment" && isVisualFile(row.content.file.filename)
  );
}

/** The rows of the two sections, in load order (newest first). */
export function mediaSections(rows: readonly ChatMessage[]): {
  readonly visual: readonly ChatMessage[];
  readonly files: readonly ChatMessage[];
} {
  const attachments = rows.filter((row) => row.content.kind === "attachment");
  return {
    visual: attachments.filter(isVisualRow),
    files: attachments.filter((row) => !isVisualRow(row)),
  };
}

export class SharedMediaStore extends Store<SharedMediaState> {
  readonly #id: string;
  readonly #queries: SharedMediaQueries;
  readonly #admit: () => boolean;
  #revision = 0;

  constructor(id: string, queries: SharedMediaQueries, admit: () => boolean) {
    super({ rows: [], allowed: false, loading: false });
    this.#id = id;
    this.#queries = queries;
    this.#admit = admit;
  }

  /**
   * Reloads. Downloads close (`allowed` false) until the new consent read
   * finishes, and only the latest load's result is kept.
   */
  async load(): Promise<void> {
    if (!this.#admit()) return;
    this.#revision += 1;
    const revision = this.#revision;
    this.setState((state) => {
      const { error: _old, ...rest } = state;
      return { ...rest, allowed: false, loading: true };
    });
    try {
      const conversation = await this.#queries.conversation(this.#id);
      const rows = await this.#queries.media(this.#id);
      if (revision !== this.#revision || !this.#admit()) return;
      this.setState({
        rows,
        allowed: conversation.consent === "allowed",
        loading: false,
      });
    } catch {
      if (revision !== this.#revision || !this.#admit()) return;
      this.setState((state) => ({
        ...state,
        loading: false,
        error: SharedMediaCopy.loadFailed,
      }));
    }
  }
}
