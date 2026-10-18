// The SDK calls behind attachments (ExampleSDK/AttachmentGateway.swift) and
// the shared-media read (ChatQueries.swift `media`).

import {
  type Client,
  type RemoteAttachment,
  XmtpError,
  standardContentType,
} from "@xmtp/react-native-sdk";

import {
  AttachmentError,
  type FilePhase,
  type VerifiedFile,
  attachmentFailure,
  mimeTypeFor,
  operationFailed,
} from "@/core/attachments.ts";
import type { AttachmentGateway } from "@/core/attachmentStore.ts";
import type { ChatMessage } from "@/core/chatMessages.ts";
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

  create(path: string, filename: string): Promise<RemoteAttachment> {
    return mapped(
      async () =>
        (
          await this.#client.attachments.create({
            kind: "path",
            path,
            filename,
            mimeType: mimeTypeFor(filename),
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

  download(file: RemoteAttachment): Promise<VerifiedFile> {
    return mapped(async () => {
      const result = await this.#client.attachments.download(file);
      return {
        path: result.path,
        ...(result.mimeType === undefined ? {} : { mimeType: result.mimeType }),
        filename: result.filename ?? file.filename ?? "File",
      };
    });
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
