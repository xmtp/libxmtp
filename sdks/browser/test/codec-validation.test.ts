import * as sdk from "@xmtp/browser-sdk/pure";
import { beforeAll, expect, test } from "vitest";

beforeAll(() => sdk.initPureWasm());

// verifies: CTYPE-014, CTYPE-026
test("standalone codecs require remote fields and accept permitted loopback URLs", () => {
  const codec = new sdk.RemoteAttachmentCodec();
  for (const url of [
    "https://example.test/file",
    "http://localhost/file",
    "http://127.0.0.1/file",
    "http://[::1]/file",
  ]) {
    const encoded = codec.encode({
      url,
      scheme: url.startsWith("https") ? "https://" : "http://",
      contentDigest:
        "abababababababababababababababababababababababababababababababab",
      secret: new Uint8Array(32).fill(1),
      salt: new Uint8Array(32).fill(2),
      nonce: new Uint8Array(12).fill(3),
    });
    expect(codec.decode(encoded).url).toBe(url);
    for (const field of [
      "contentDigest",
      "secret",
      "salt",
      "nonce",
      "scheme",
    ]) {
      const parameters = new Map(encoded.parameters);
      parameters.delete(field);
      expect(() => codec.decode({ ...encoded, parameters })).toThrow();
    }
  }
  const nested = new sdk.TextCodec().encode("body");
  const reply = {
    kind: "reply",
    reference: "a".repeat(64),
    content: nested,
  } as const;
  const encoded = new sdk.ReplyCodec().encode(reply);
  expect(new sdk.ReplyCodec().decode(encoded).content).toEqual(nested);
  expect(encoded).toEqual(sdk.encodeStandard(reply));
});

// verifies: CTYPE-030
test("Actions expiry decode checks both fields at the signed range limits", () => {
  const codec = new sdk.ActionsCodec();
  for (const [timestamp, valid] of [
    ["1677-09-21T00:12:43.145224192Z", true],
    ["2262-04-11T23:47:16.854775807Z", true],
    ["1677-09-21T00:12:43.145224191Z", false],
    ["2262-04-11T23:47:16.854775808Z", false],
  ] as const) {
    for (const nested of [false, true]) {
      const value = {
        id: "choices",
        description: "Choose",
        actions: [
          {
            id: "one",
            label: "One",
            ...(nested ? { expiresAt: timestamp } : {}),
          },
        ],
        ...(!nested ? { expiresAt: timestamp } : {}),
      };
      const encoded = {
        type: codec.type,
        parameters: new Map<string, string>(),
        content: new TextEncoder().encode(JSON.stringify(value)),
      };
      if (valid) {
        const decoded = codec.decode(encoded);
        expect(
          (nested ? decoded.actions[0].expiresAt : decoded.expiresAt)?.ns,
        ).toBe(
          timestamp.startsWith("1677")
            ? -9223372036854775808n
            : 9223372036854775807n,
        );
      } else
        expect(() => codec.decode(encoded)).toThrow(
          sdk.XmtpError.CodecDecodeFailed,
        );
    }
  }
  const value = {
    id: "choices",
    description: "Choose",
    actions: [{ id: "one", label: "One" }],
    expiresAt: new sdk.Timestamp(-9223372036854775808n),
  };
  expect(() => codec.encode(value)).toThrow(sdk.XmtpError.InvalidInput);
});

const fromBase64 = (value: string) =>
  Uint8Array.from(atob(value), (c) => c.charCodeAt(0));
const concatenate = (parts: Uint8Array[]) => {
  const bytes = new Uint8Array(parts.reduce((n, part) => n + part.length, 0));
  let offset = 0;
  for (const part of parts) {
    bytes.set(part, offset);
    offset += part.length;
  }
  return bytes;
};
// Independent gzip members: each expands to exactly 1 MiB, then one extra byte.
const member = fromBase64(
  "H4sIAAAAAAAAE+3BMQEAAADCoGzrX8oQvkABAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAHwGyb72gQAAEAA=",
);
const extra = fromBase64("H4sIAAAAAAAAE3MEAIue2dMBAAAA");
const wire = (content: Uint8Array, algorithm: number) =>
  concatenate([
    sdk.encodeEncodedContent({ ...new sdk.TextCodec().encode(""), content }),
    new Uint8Array([0x28, algorithm]),
  ]);

// verifies: CTYPE-024, CTYPE-025, CTYPE-026
test("standalone and nested decoding enforce the inclusive output bound", () => {
  const exact = concatenate(Array.from({ length: 16 }, () => member));
  expect(sdk.decodeEncodedContent(wire(exact, 1)).content.length).toBe(
    16_777_216,
  );
  const overflow = wire(concatenate([exact, extra]), 1);
  expect(() => sdk.decodeEncodedContent(overflow)).toThrow();
  const reply = new sdk.ReplyCodec().encode({
    kind: "reply",
    reference: "a".repeat(64),
    content: new sdk.TextCodec().encode("body"),
  });
  expect(() =>
    new sdk.ReplyCodec().decode({ ...reply, content: overflow }),
  ).toThrow();
  expect(() =>
    sdk.decodeEncodedContent(wire(new Uint8Array([1]), 99)),
  ).toThrow();
});

// verifies: CTYPE-009
test("standalone codec failures have four distinct typed outcomes", () => {
  const text = new sdk.TextCodec().encode("body");
  const attempts = [
    [
      "InvalidInput",
      () =>
        new sdk.ActionsCodec().encode({
          id: "choices",
          description: "Choose",
          actions: [],
        }),
    ],
    [
      "CodecDecodeFailed",
      () => sdk.decodeStandard({ ...text, content: new Uint8Array([0xff]) }),
    ],
    [
      "CodecNotFound",
      () =>
        sdk.decodeStandard({
          ...text,
          type: { ...text.type, authorityId: "other.example" },
        }),
    ],
    [
      "MalformedEnvelope",
      () => sdk.decodeEncodedContent(new Uint8Array([0xff])),
    ],
  ] as const;
  for (const [code, operation] of attempts) {
    try {
      operation();
      throw new Error("Expected codec failure");
    } catch (error) {
      expect(error).toBeInstanceOf(sdk.XmtpError[code]);
      if (!(error instanceof sdk.XmtpError[code])) throw error;
      expect(error.details).toMatchObject({
        code,
        category: "input",
        retryable: false,
      });
      expect(error.details.message).not.toBe("");
    }
  }
});

// verifies: CTYPE-014, CTYPE-026
test("legacy schemes decode without changing stored parameters", () => {
  const codec = new sdk.RemoteAttachmentCodec();
  for (const [url, bare, canonical] of [
    ["https://example.test/file", "https", "https://"],
    ["http://localhost/file", "http", "http://"],
  ] as const) {
    const value = {
      url,
      scheme: canonical,
      contentDigest: "ab".repeat(32),
      secret: new Uint8Array(32).fill(1),
      salt: new Uint8Array(32).fill(2),
      nonce: new Uint8Array(12).fill(3),
    };
    const encoded = codec.encode(value);
    const legacy = {
      ...encoded,
      parameters: new Map(encoded.parameters).set("scheme", bare),
    };
    const raw = sdk.encodeEncodedContent(legacy);
    const decoded = codec.decode(legacy);
    expect(decoded.scheme).toBe(canonical);
    expect(legacy.parameters.get("scheme")).toBe(bare);
    expect(sdk.encodeEncodedContent(legacy)).toEqual(raw);
    expect(codec.encode(decoded).parameters.get("scheme")).toBe(canonical);
    expect(() => codec.encode({ ...value, scheme: bare })).toThrow(
      sdk.XmtpError.InvalidInput,
    );
    if (bare === "http")
      expect(() =>
        codec.decode({
          ...legacy,
          content: new TextEncoder().encode("http://example.test/file"),
        }),
      ).toThrow(sdk.XmtpError.CodecDecodeFailed);
    const nested = new sdk.ReplyCodec().encode({
      kind: "reply",
      reference: "a".repeat(64),
      content: legacy,
    });
    expect(
      codec.decode(new sdk.ReplyCodec().decode(nested).content).scheme,
    ).toBe(canonical);
  }
});
