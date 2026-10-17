import {
  decryptBytes,
  decryptEncodedContent,
  encryptBytes,
  encryptEncodedContent,
} from "@xmtp/browser-sdk";
import {
  AttachmentCodec,
  decodeEncodedContent,
  encodeEncodedContent,
  initPureWasm,
} from "@xmtp/browser-sdk/pure";
import { beforeAll, expect, test } from "vitest";

import { legacyEncryptedAttachment } from "./fixtures/legacy-encrypted-attachment";

beforeAll(() => initPureWasm());

const attachment = {
  filename: "test.txt",
  mimeType: "text/plain",
  content: new TextEncoder().encode("foo"),
};

// verifies: CTYPE-015
test("the public worker decrypts the legacy TS attachment payload", async () => {
  const fixture = legacyEncryptedAttachment;
  const content = await decryptEncodedContent({
    ciphertext: new Uint8Array(fixture.payload),
    keys: {
      secret: new Uint8Array(fixture.secret),
      salt: new Uint8Array(fixture.salt),
      nonce: new Uint8Array(fixture.nonce),
      digest: fixture.digest,
      length: BigInt(fixture.payload.length),
    },
  });
  expect(new AttachmentCodec().decode(decodeEncodedContent(content))).toEqual(
    attachment,
  );
});

// verifies: CTYPE-015
test("public attachment encryption preserves fields, key sizes, and fresh randomness", async () => {
  const codec = new AttachmentCodec();
  const bytes = encodeEncodedContent(codec.encode(attachment));
  const first = await encryptEncodedContent(bytes);
  const second = await encryptEncodedContent(bytes);
  for (const encrypted of [first, second]) {
    expect(encrypted.keys.secret).toHaveLength(32);
    expect(encrypted.keys.salt).toHaveLength(32);
    expect(encrypted.keys.nonce).toHaveLength(12);
    expect(encrypted.keys.length).toBe(BigInt(encrypted.ciphertext.length));
    expect(encrypted.ciphertext).toHaveLength(bytes.length + 16);
    expect(encrypted.keys.digest).toMatch(/^[0-9a-f]{64}$/);
    expect(
      codec.decode(
        decodeEncodedContent(await decryptEncodedContent(encrypted)),
      ),
    ).toEqual(attachment);
  }
  expect(first.keys.secret).not.toEqual(second.keys.secret);
  expect(first.keys.salt).not.toEqual(second.keys.salt);
  expect(first.keys.nonce).not.toEqual(second.keys.nonce);
  expect(first.keys.digest).not.toBe(second.keys.digest);
});

// verifies: CTYPE-015
test.each(["digest", "secret", "ciphertext"] as const)(
  "the public worker rejects an attachment with changed %s",
  async (field) => {
    const encrypted = await encryptEncodedContent(
      encodeEncodedContent(new AttachmentCodec().encode(attachment)),
    );
    const ciphertext = encrypted.ciphertext.slice();
    const keys = { ...encrypted.keys, secret: encrypted.keys.secret.slice() };
    if (field === "digest") keys.digest = "0".repeat(64);
    if (field === "secret") keys.secret[0] ^= 1;
    if (field === "ciphertext") ciphertext[0] ^= 1;
    await expect(decryptEncodedContent({ ciphertext, keys })).rejects.toThrow();
  },
);

test("public byte encryption returns exact bytes and rejects changed ciphertext", async () => {
  const encrypted = await encryptBytes(attachment.content);
  expect(await decryptBytes(encrypted.ciphertext, encrypted.keys)).toEqual(
    attachment.content,
  );
  const ciphertext = encrypted.ciphertext.slice();
  ciphertext[0] ^= 1;
  await expect(decryptBytes(ciphertext, encrypted.keys)).rejects.toThrow();
});
