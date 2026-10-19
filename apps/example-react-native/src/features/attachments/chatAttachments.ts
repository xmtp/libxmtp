// The attachment store of one open chat. The composer slots and the file
// cards share it through the chat runtime. While any of them is mounted it is
// attached: it loaded once, and a consent change (or a lagged stream) drops
// every download.

import { useEffect } from "react";

import type { AttachmentStore } from "@/core/attachmentStore.ts";
import {
  consentAllowed,
  createAttachmentStore,
} from "@/features/attachments/attachmentServices";
import type { ChatRuntime } from "@/features/chat/chatRuntime";

class ChatAttachments {
  readonly store: AttachmentStore;
  readonly #runtime: ChatRuntime;
  #attached = 0;
  #loaded = false;
  #stop: (() => void) | undefined;

  constructor(runtime: ChatRuntime) {
    this.#runtime = runtime;
    const { session, conversationId } = runtime;
    this.store = createAttachmentStore(session, {
      conversationId,
      admit: () => this.#attached > 0,
      allowed: () => runtime.allowed,
      authorizeDownload: consentAllowed(session, conversationId),
      changed: () => runtime.timeline.refresh(),
    });
  }

  attach(): () => void {
    this.#attached += 1;
    if (this.#attached === 1) {
      this.#stop = this.#runtime.session.streams.onEvent((event) => {
        if (event.kind === "changed") this.store.invalidateDownloads();
      });
      if (!this.#loaded) {
        this.#loaded = true;
        void this.store.load();
      }
    }
    let detached = false;
    return () => {
      if (detached) return;
      detached = true;
      this.#attached -= 1;
      if (this.#attached === 0) {
        this.#stop?.();
        this.#stop = undefined;
      }
    };
  }
}

const chats = new WeakMap<ChatRuntime, ChatAttachments>();

function chatAttachments(runtime: ChatRuntime): ChatAttachments {
  let value = chats.get(runtime);
  if (value === undefined) {
    value = new ChatAttachments(runtime);
    chats.set(runtime, value);
  }
  return value;
}

/** The chat's attachment store, attached while the calling component is mounted. */
export function useChatAttachments(runtime: ChatRuntime): AttachmentStore {
  const value = chatAttachments(runtime);
  useEffect(() => value.attach(), [value]);
  return value.store;
}
