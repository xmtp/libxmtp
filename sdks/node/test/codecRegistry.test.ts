import { createRegisteredClient, createSigner } from "@test/helpers";
import type { EncodedContent } from "@xmtp/node-sdk";
import { expect, test } from "vitest";

// verifies: CTYPE-001, CTYPE-009
test("registry matches authority, type, and major and ignores minor", async () => {
  const type = {
    authorityId: "example.org",
    typeId: "note",
    versionMajor: 1,
    versionMinor: 0,
  };
  const codec = {
    type,
    encode: (value: string): EncodedContent => ({
      type,
      content: new TextEncoder().encode(value),
      parameters: new Map(),
    }),
    decode: (encoded: EncodedContent) =>
      new TextDecoder().decode(encoded.content),
  };
  const client = await createRegisteredClient(createSigner().signer, {
    codecs: [codec],
  });
  try {
    for (const [changed, matched] of [
      [{ ...type, versionMinor: 9 }, true],
      [{ ...type, authorityId: "other.org" }, false],
      [{ ...type, typeId: "other" }, false],
      [{ ...type, versionMajor: 2 }, false],
    ] as const) {
      const encoded = {
        ...codec.encode("value"),
        type: changed,
        fallback: "kept",
      };
      const content = await client.decodeContent(encoded);
      expect(content).toMatchObject(
        matched
          ? { kind: "custom", value: "value", encoded }
          : { kind: "unknown", encoded, error: { code: "CodecNotFound" } },
      );
    }
  } finally {
    await client.end();
  }
});

// verifies: CTYPE-023, CTYPE-024
test("send options write the shared zlib and gzip frames", async () => {
  const client = await createRegisteredClient(createSigner().signer);
  try {
    const group = await client.conversations.createGroup([]);
    for (const [compression, magic] of [
      ["deflate", [0x78, 0x9c]],
      ["gzip", [0x1f, 0x8b]],
    ] as const) {
      const text = compression.repeat(100);
      const id = await group.sendText(text, { compression });
      const message = await client.conversations.getMessageById(id);
      expect(message?.content).toEqual({ kind: "text", value: text });
      expect(
        message?.rawBytes.some(
          (byte, i, bytes) => byte === magic[0] && bytes[i + 1] === magic[1],
        ),
      ).toBe(true);
    }
  } finally {
    await client.end();
  }
});
