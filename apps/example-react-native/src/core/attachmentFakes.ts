// Test support for the attachment stores: remote files, in-memory preference
// files, an SDK-like gateway and a conversation that queues file messages.
// Only tests import it.

import type { RemoteAttachment } from "@xmtp/react-native-sdk";

import {
  AttachmentCopy,
  AttachmentError,
  type DraftPreferences,
  type FilePhase,
  type VerifiedFile,
  sameFile,
} from "./attachments.ts";
import type { AttachmentGateway } from "./attachmentStore.ts";
import { FakeConversation, OWN } from "./chatFakes.ts";
import type { OutboundContent } from "./chatHistory.ts";

export function remoteFile(name = "notes.txt", length = 12): RemoteAttachment {
  return {
    url: `https://files.example/${name}`,
    contentDigest: `digest-${name}`,
    secret: new Uint8Array([1, 2, 3]),
    salt: new Uint8Array([4, 5]),
    nonce: new Uint8Array([6]),
    scheme: "https://",
    contentLength: length,
    filename: name,
  };
}

/** In-memory preference files, JSON round-tripped like the device store. */
export class MemoryPreferences implements DraftPreferences {
  readonly files = new Map<string, string>();

  readJson(name: string): unknown {
    const text = this.files.get(name);
    return text === undefined ? undefined : (JSON.parse(text) as unknown);
  }

  writeJson(name: string, value: unknown): void {
    this.files.set(name, JSON.stringify(value));
  }

  remove(name: string): void {
    this.files.delete(name);
  }

  names(prefix: string): readonly string[] {
    return [...this.files.keys()].filter((name) => name.startsWith(prefix));
  }
}

/** The SDK's attachment calls, in memory. */
export class FakeGateway implements AttachmentGateway {
  limit: bigint | undefined = 1_000_000n;
  /** SDK-held uploads that are not complete. */
  pendingFiles: RemoteAttachment[] = [];
  created: { path: string; filename: string; mimeType: string }[] = [];
  uploads: RemoteAttachment[] = [];
  downloads: RemoteAttachment[] = [];
  discarded: RemoteAttachment[] = [];
  createError: Error | undefined;
  uploadError: Error | undefined;
  uploadPhase: FilePhase = "complete";
  downloadError: Error | undefined;
  /** Holds downloads until released. */
  downloadGate: Promise<void> | undefined;

  capability(): bigint | undefined {
    return this.limit;
  }

  create(
    path: string,
    filename: string,
    mimeType: string,
  ): Promise<RemoteAttachment> {
    this.created.push({ path, filename, mimeType });
    if (this.createError !== undefined) return Promise.reject(this.createError);
    const file = remoteFile(filename);
    this.pendingFiles.push(file);
    return Promise.resolve(file);
  }

  pending(): Promise<RemoteAttachment[]> {
    return Promise.resolve([...this.pendingFiles]);
  }

  upload(file: RemoteAttachment): Promise<FilePhase> {
    this.uploads.push(file);
    if (this.uploadError !== undefined) {
      const error = this.uploadError;
      this.uploadError = undefined;
      return Promise.reject(error);
    }
    if (this.uploadPhase === "complete") {
      this.pendingFiles = this.pendingFiles.filter(
        (entry) => !sameFile(entry, file),
      );
    }
    return Promise.resolve(this.uploadPhase);
  }

  async download(file: RemoteAttachment): Promise<VerifiedFile> {
    this.downloads.push(file);
    if (this.downloadGate !== undefined) await this.downloadGate;
    if (this.downloadError !== undefined) throw this.downloadError;
    return {
      path: `/sdk/attachments/${file.filename ?? "file"}`,
      mimeType: "text/plain",
      filename: file.filename ?? "File",
    };
  }

  discard(file: RemoteAttachment): Promise<void> {
    this.discarded.push(file);
    this.pendingFiles = this.pendingFiles.filter(
      (entry) => !sameFile(entry, file),
    );
    return Promise.resolve();
  }
}

/** A failure the SDK would call retryable or not. */
export function attachmentError(retryable: boolean): AttachmentError {
  return new AttachmentError(AttachmentCopy.operationFailed, retryable);
}

/** A conversation whose queued file messages appear as unpublished rows. */
export class FileConversation extends FakeConversation {
  override queue(
    conversationId: string,
    content: OutboundContent,
  ): Promise<string> {
    const queued = super.queue(conversationId, content);
    if (content.kind !== "attachment") return queued;
    return queued.then((id) => {
      const sentAtNs = BigInt(this.unpublished.length + 1) * 1_000_000_000n;
      this.unpublished.push({
        id,
        conversationId,
        sender: OWN,
        sentAtNs,
        insertedAtNs: sentAtNs,
        delivery: "queued",
        content: { kind: "attachment", file: content.file },
        reactions: [],
      });
      return id;
    });
  }
}
