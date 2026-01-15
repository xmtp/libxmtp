import {
  Attachment,
  AttachmentCodec,
  RemoteAttachmentCodec,
} from "@xmtp/content-type-remote-attachment";
import { toHex } from "viem";
import { afterAll, describe, expect, it } from "vitest";
import init, { decryptAttachment, encryptAttachment } from "../";

await init();

describe("RemoteAttachment encryption compatibility", () => {
  const originalFetch = globalThis.fetch;

  afterAll(() => {
    globalThis.fetch = originalFetch;
  });

  const mockFetch = (responseData: ArrayBufferLike) => {
    globalThis.fetch = async () =>
      ({
        ok: true,
        status: 200,
        statusText: "OK",
        arrayBuffer: async () => responseData,
      }) as Response;
  };

  const testContent = "foo";
  const encodedContent = new TextEncoder().encode(testContent);
  const testFilename = "test.txt";
  const testMimeType = "text/plain";

  it("should decrypt TS encrypted payload with WASM", async () => {
    const encrypted = await RemoteAttachmentCodec.encodeEncrypted(
      {
        filename: testFilename,
        mimeType: testMimeType,
        data: encodedContent,
      },
      new AttachmentCodec(),
    );

    const decrypted = decryptAttachment(encrypted.payload, {
      url: "https://example.com/test",
      contentDigest: encrypted.digest,
      secret: encrypted.secret,
      salt: encrypted.salt,
      nonce: encrypted.nonce,
      scheme: "https",
      contentLength: encrypted.payload.byteLength,
      filename: testFilename,
    });

    expect(new TextDecoder().decode(decrypted.content)).toBe(testContent);
    expect(decrypted.filename).toBe(testFilename);
    expect(decrypted.mimeType).toBe(testMimeType);
  });

  it("should decrypt WASM encrypted payload with TS", async () => {
    const encrypted = encryptAttachment({
      filename: testFilename,
      mimeType: testMimeType,
      content: encodedContent,
    });

    mockFetch(encrypted.payload.buffer);

    const decrypted = await RemoteAttachmentCodec.load<Attachment>(
      {
        url: "https://example.com/wasm-encrypted",
        contentDigest: encrypted.contentDigest,
        secret: encrypted.secret,
        salt: encrypted.salt,
        nonce: encrypted.nonce,
        scheme: "https",
        contentLength: encrypted.contentLength,
        filename: testFilename,
      },
      {
        codecFor: () => new AttachmentCodec(),
      },
    );

    expect(new TextDecoder().decode(decrypted.data)).toBe(testContent);
    expect(decrypted.filename).toBe(testFilename);
    expect(decrypted.mimeType).toBe(testMimeType);
  });

  it("should fail with wrong content digest", () => {
    const encrypted = encryptAttachment({
      filename: testFilename,
      mimeType: testMimeType,
      content: encodedContent,
    });

    expect(() =>
      decryptAttachment(encrypted.payload, {
        url: "https://example.com/test",
        contentDigest: "wrong_digest",
        secret: encrypted.secret,
        salt: encrypted.salt,
        nonce: encrypted.nonce,
        scheme: "https",
        contentLength: encrypted.payload.byteLength,
        filename: testFilename,
      }),
    ).toThrow("content digest mismatch");
  });

  it("should fail with wrong secret", () => {
    const encrypted = encryptAttachment({
      filename: testFilename,
      mimeType: testMimeType,
      content: encodedContent,
    });

    const wrongSecret = new Uint8Array(32);
    crypto.getRandomValues(wrongSecret);

    expect(() =>
      decryptAttachment(encrypted.payload, {
        url: "https://example.com/test",
        contentDigest: encrypted.contentDigest,
        secret: wrongSecret,
        salt: encrypted.salt,
        nonce: encrypted.nonce,
        scheme: "https",
        contentLength: encrypted.payload.byteLength,
        filename: testFilename,
      }),
    ).toThrow();
  });

  it("should fail with corrupted payload", () => {
    const encrypted = encryptAttachment({
      filename: testFilename,
      mimeType: testMimeType,
      content: encodedContent,
    });

    const corruptedPayload = new Uint8Array(encrypted.payload);
    corruptedPayload[0] ^= 0xff;

    expect(() =>
      decryptAttachment(corruptedPayload, {
        url: "https://example.com/test",
        contentDigest: encrypted.contentDigest,
        secret: encrypted.secret,
        salt: encrypted.salt,
        nonce: encrypted.nonce,
        scheme: "https",
        contentLength: corruptedPayload.byteLength,
        filename: testFilename,
      }),
    ).toThrow();
  });

  it("should create a 32-byte secret", () => {
    const attachment = {
      filename: "test.txt",
      mimeType: "text/plain",
      content: encodedContent,
    };
    const encrypted = encryptAttachment(attachment);
    expect(encrypted.secret.length).toBe(32);
  });

  it("should create a 32-byte salt", () => {
    const attachment = {
      filename: "test.txt",
      mimeType: "text/plain",
      content: encodedContent,
    };
    const encrypted = encryptAttachment(attachment);
    expect(encrypted.salt.length).toBe(32);
  });

  it("should create a 12-byte nonce", () => {
    const attachment = {
      filename: "test.txt",
      mimeType: "text/plain",
      content: encodedContent,
    };
    const encrypted = encryptAttachment(attachment);
    expect(encrypted.nonce.length).toBe(12);
  });

  it("should produce unique encryption each time", () => {
    const attachment = {
      filename: "test.txt",
      mimeType: "text/plain",
      content: encodedContent,
    };

    const encrypted1 = encryptAttachment(attachment);
    const encrypted2 = encryptAttachment(attachment);

    expect(toHex(encrypted1.secret)).not.toBe(toHex(encrypted2.secret));
    expect(toHex(encrypted1.salt)).not.toBe(toHex(encrypted2.salt));
    expect(toHex(encrypted1.nonce)).not.toBe(toHex(encrypted2.nonce));
    expect(encrypted1.contentDigest).not.toBe(encrypted2.contentDigest);
  });
});
