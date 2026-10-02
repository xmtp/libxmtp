import { readFile } from "node:fs/promises";

import {
  AttachmentCodec,
  encodeEncodedContent,
  encryptEncodedContent,
  type Attachment,
  type Client,
  type EncryptionKeys,
  type RemoteAttachment,
} from "@xmtp/node-sdk";

/** Bytes and Rust-generated keys for an app-owned upload. */
export type HostedAttachment = EncryptionKeys & {
  readonly payload: Uint8Array;
  readonly filename?: string;
};
/** Upload encrypted bytes to app-owned storage and return its URL. */
export type AttachmentUploadCallback = (
  attachment: HostedAttachment,
) => Promise<string>;

/** Download and verify an attachment through Rust. */
export async function downloadRemoteAttachment(
  client: Client,
  remoteAttachment: RemoteAttachment,
): Promise<Attachment> {
  const downloaded = await client.attachments.download(remoteAttachment);
  return {
    content: new Uint8Array(await readFile(downloaded.path)),
    mimeType: downloaded.mimeType ?? "application/octet-stream",
    filename: downloaded.filename,
  };
}

/** Use Rust-generated encryption material with an app-owned URL. */
export function createRemoteAttachment(
  encrypted: HostedAttachment,
  fileUrl: string,
): RemoteAttachment {
  const url = new URL(fileUrl);
  return {
    url: url.toString(),
    scheme: url.protocol.replace(/:$/, ""),
    contentDigest: encrypted.digest,
    contentLength: encrypted.payload.length,
    filename: encrypted.filename,
    secret: encrypted.secret,
    salt: encrypted.salt,
    nonce: encrypted.nonce,
  };
}

/** Create and upload a file. The optional callback selects app-owned hosting. */
export async function createRemoteAttachmentFromFile(
  client: Client,
  file: File,
  uploadCallback?: AttachmentUploadCallback,
): Promise<RemoteAttachment> {
  const bytes = new Uint8Array(await file.arrayBuffer());
  if (!uploadCallback) {
    const pending = await client.attachments.create({
      kind: "bytes",
      bytes,
      filename: file.name,
      mimeType: file.type || "application/octet-stream",
    });
    await pending.upload();
    return pending.remoteAttachment;
  }
  const encoded = new AttachmentCodec().encode({
    content: bytes,
    filename: file.name,
    mimeType: file.type || "application/octet-stream",
  });
  const encrypted = await encryptEncodedContent(encodeEncodedContent(encoded));
  const hosted = {
    ...encrypted.keys,
    payload: encrypted.ciphertext,
    filename: file.name,
  };
  return createRemoteAttachment(hosted, await uploadCallback(hosted));
}
