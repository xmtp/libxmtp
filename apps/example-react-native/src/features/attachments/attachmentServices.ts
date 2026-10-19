// Per-session attachment services: the SDK gateway, the saved drafts, and the
// staging and export directories. A store per screen comes from
// `createAttachmentStore`.

import type { RemoteAttachment } from "@xmtp/react-native-sdk";

import { AttachmentDraftStore } from "@/core/attachments.ts";
import { AttachmentStore } from "@/core/attachmentStore.ts";
import { errorLogText } from "@/core/errorLog.ts";
import { SdkAttachmentGateway } from "@/features/attachments/sdkAttachmentGateway";
import { type ChatServices, chatServices } from "@/features/chat/chatServices";
import {
  removeAbandonedStaging,
  removeStaged,
  stagingDirectory,
} from "@/platform/attachmentFiles";
import { removePath } from "@/platform/files";
import type { ActiveSession } from "@/session";

export interface AttachmentServices {
  readonly session: ActiveSession;
  readonly chat: ChatServices;
  readonly gateway: SdkAttachmentGateway;
  readonly drafts: AttachmentDraftStore;
  /** `<profile>/temp/attachment-staging`. */
  readonly staging: string;
  /** `<profile>/exports`. */
  readonly exports: string;
  /** Draft IDs whose send runs in any store of the session. */
  readonly activeSends: Set<string>;
}

const services = new WeakMap<ActiveSession, AttachmentServices>();

/** The attachment services of `session`, created on first use. */
export function attachmentServices(session: ActiveSession): AttachmentServices {
  const existing = services.get(session);
  if (existing !== undefined) return existing;
  const staging = stagingDirectory(session.paths.temp);
  try {
    // Nothing stages before the first use in this session.
    removeAbandonedStaging(staging);
  } catch (error) {
    console.warn("Cannot remove abandoned staged files", errorLogText(error));
  }
  const value: AttachmentServices = {
    session,
    chat: chatServices(session),
    gateway: new SdkAttachmentGateway(session.client),
    drafts: new AttachmentDraftStore(session.preferences),
    staging,
    exports: session.paths.exports,
    activeSends: new Set(),
  };
  // Exported copies never outlive the session (sign out, reset).
  session.scope.addFinalCleanup(() => {
    try {
      removePath(value.exports);
    } catch (error) {
      console.warn("Cannot remove exported files", errorLogText(error));
    }
  });
  services.set(session, value);
  return value;
}

export interface AttachmentStoreOptions {
  /** The chat that stages and sends; undefined on recovery. */
  readonly conversationId?: string;
  readonly admit: () => boolean;
  readonly allowed: () => boolean;
  /** Re-reads the conversation's consent before a download. */
  readonly authorizeDownload?: () => Promise<boolean>;
  /** After a draft's message was published. */
  readonly changed?: () => Promise<void>;
}

/** A store for one screen: a chat, Media and files, or Attachment recovery. */
export function createAttachmentStore(
  session: ActiveSession,
  options: AttachmentStoreOptions,
): AttachmentStore {
  const value = attachmentServices(session);
  const { queries, coordinator, newId, now } = value.chat;
  return new AttachmentStore({
    gateway: value.gateway,
    drafts: value.drafts,
    coordinator,
    ...(options.conversationId === undefined
      ? {}
      : { conversationId: options.conversationId }),
    admit: () => options.admit() && session.isCurrent(),
    acceptsSession: () => session.isCurrent(),
    allowed: options.allowed,
    ...(options.authorizeDownload === undefined
      ? {}
      : { authorizeDownload: options.authorizeDownload }),
    sendFile: (conversationId: string, file: RemoteAttachment) =>
      queries.queue(conversationId, { kind: "attachment", file }),
    activeSends: value.activeSends,
    published: async (conversationId, messageId) => {
      const message = await queries.message(messageId);
      return (
        message?.conversationId === conversationId &&
        message.delivery === "published"
      );
    },
    removeStaged: (path) => {
      try {
        removeStaged(path, value.staging);
      } catch (error) {
        console.warn("Cannot remove a staged file", errorLogText(error));
      }
    },
    ...(options.changed === undefined ? {} : { changed: options.changed }),
    newId,
    now,
  });
}

/** Re-reads a conversation's consent (`authorizeDownload`). */
export function consentAllowed(
  session: ActiveSession,
  conversationId: string,
): () => Promise<boolean> {
  const { queries } = chatServices(session);
  return async () =>
    (await queries.conversation(conversationId)).consent === "allowed";
}
