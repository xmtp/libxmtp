// The SDK calls behind attachments and the shared-media read.

import {
  type Client,
  type RemoteAttachment,
  XmtpError,
  standardContentType,
} from "@xmtp/react-native-sdk";
import { File } from "expo-file-system";

import {
  AttachmentError,
  type FilePhase,
  type VerifiedFile,
  attachmentFailure,
  operationFailed,
} from "@/core/attachments.ts";
import type { AttachmentGateway } from "@/core/attachmentStore.ts";
import type { ChatMessage } from "@/core/chatMessages.ts";
import { pathToUri } from "@/core/paths.ts";
import { SendError } from "@/core/sendJournal.ts";
import { chatMessage } from "@/features/chat/sdkChatQueries";

/** Maps SDK errors to the copy the cards and composer show. */
export function mapAttachmentError(error: unknown): AttachmentError {
  if (error instanceof AttachmentError) return error;
  if (error instanceof XmtpError.Attachment) {
    return attachmentFailure(error.attachmentFailure, error.details.retryable);
  }
  return operationFailed();
}

async function mapped<T>(work: () => Promise<T>): Promise<T> {
  try {
    return await work();
  } catch (error) {
    throw mapAttachmentError(error);
  }
}

/**
 * Debug-only hooks for checks the shared backend cannot produce on demand
 * (`dev/attachment-probe` sets them): a smaller upload limit, and uploads
 * that wait until released, so the app can be stopped mid-upload.
 */
export const attachmentDebug: {
  limit: bigint | undefined;
  holdUploads: boolean;
} = { limit: undefined, holdUploads: false };

async function heldUpload(): Promise<void> {
  while (__DEV__ && attachmentDebug.holdUploads) {
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
}

export class SdkAttachmentGateway implements AttachmentGateway {
  readonly #client: Client;
  readonly #downloads = new Set<(file: RemoteAttachment) => void>();

  constructor(client: Client) {
    this.#client = client;
  }

  capability(): bigint | undefined {
    try {
      if (!this.#client.attachments.offered) return undefined;
      const limit =
        this.#client.serverConfiguration.attachments?.maxUploadBytes;
      return __DEV__ &&
        limit !== undefined &&
        attachmentDebug.limit !== undefined
        ? attachmentDebug.limit
        : limit;
    } catch {
      // An offline open may have no server configuration.
      return undefined;
    }
  }

  create(
    path: string,
    filename: string,
    mimeType: string,
  ): Promise<RemoteAttachment> {
    return mapped(
      async () =>
        (
          await this.#client.attachments.create({
            kind: "path",
            path,
            filename,
            mimeType,
          })
        ).remoteAttachment,
    );
  }

  pending(): Promise<RemoteAttachment[]> {
    return mapped(async () =>
      (await this.#client.attachments.listPending()).map(
        (entry) => entry.remoteAttachment,
      ),
    );
  }

  upload(file: RemoteAttachment): Promise<FilePhase> {
    return mapped(async () => {
      const pending = await this.#client.attachments.pending(file);
      await heldUpload();
      await pending.upload();
      const status = await pending.status();
      switch (status.kind) {
        case "waiting":
        case "uploading":
        case "complete":
          return status.kind;
        case "failed":
          throw attachmentFailure(status.value);
      }
    });
  }

  hasLocal(file: RemoteAttachment): Promise<boolean> {
    return mapped(async () => {
      const path = await this.#client.attachments.localPath(file);
      return new File(pathToUri(path)).exists;
    });
  }

  download(file: RemoteAttachment): Promise<VerifiedFile> {
    return mapped(async () => {
      const result = await this.#client.attachments.download(file);
      for (const listener of this.#downloads) listener(file);
      return {
        path: result.path,
        ...(result.mimeType === undefined ? {} : { mimeType: result.mimeType }),
        filename: result.filename ?? file.filename ?? "File",
      };
    });
  }

  watchDownloads(listener: (file: RemoteAttachment) => void): () => void {
    this.#downloads.add(listener);
    return () => {
      this.#downloads.delete(listener);
    };
  }

  discard(file: RemoteAttachment): Promise<void> {
    return mapped(() => this.#client.attachments.deleteLocal(file));
  }
}

/** The 50 newest file messages of a conversation, newest first. */
export async function mediaMessages(
  client: Client,
  conversationId: string,
): Promise<ChatMessage[]> {
  const conversation = await client.conversations.getById(conversationId);
  if (conversation === undefined) throw new SendError("unavailable");
  const messages = await conversation.messages({
    limit: 50,
    direction: "descending",
    contentTypes: [standardContentType("remoteAttachment")],
  });
  return messages.map(chatMessage);
}
