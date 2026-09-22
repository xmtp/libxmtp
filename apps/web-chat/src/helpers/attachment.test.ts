import { encryptAttachment, type Attachment } from "@xmtp/browser-sdk";
import { describe, expect, it } from "vitest";

import { downloadRemoteAttachment } from "./attachment";

describe("remote attachments", () => {
  it("decrypts bytes downloaded from a blob URL", async () => {
    const original = new TextEncoder().encode("hello from xmtp.chat");
    const attachment: Attachment = {
      filename: "hello.txt",
      mimeType: "text/plain",
      content: original,
    };
    const encrypted = await encryptAttachment(attachment);
    const payload = Uint8Array.from(encrypted.payload).buffer;
    const url = URL.createObjectURL(new Blob([payload]));
    try {
      const downloaded = await downloadRemoteAttachment({
        url,
        contentDigest: encrypted.contentDigest,
        salt: encrypted.salt,
        nonce: encrypted.nonce,
        secret: encrypted.secret,
        scheme: "blob:",
        contentLength: encrypted.payload.length,
        filename: attachment.filename,
      });
      expect(downloaded.content).toEqual(original);
    } finally {
      URL.revokeObjectURL(url);
    }
  });
});
