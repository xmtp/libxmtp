import { describe, expect, it } from "vitest";

import * as P from "../../../../target/sdk-generated/typescript-wasm/public-values.gen";
import * as B from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk";

// The value pass delegates live handles to a target adapter. These two sentinels
// check that a nested handle crosses that adapter without being copied.
const backend = { [P.objectBrand]: "Backend" } as P.Backend;
const rawBackend = {} as B.BackendLike;
const projection = new Proxy({} as P.ObjectProjection, {
  get(_target, name) {
    if (name === "isBackend") return (value: unknown) => value === backend;
    if (name === "lowerBackend")
      return (value: unknown) => {
        expect(value).toBe(backend);
        return rawBackend;
      };
    if (name === "liftBackend")
      return (value: unknown) => {
        expect(value).toBe(rawBackend);
        return backend;
      };
    return () => {
      throw new Error(`unexpected object projection ${String(name)}`);
    };
  },
});
const type: P.ContentTypeId = {
  authorityId: "example.test",
  typeId: "data",
  versionMajor: 1,
  versionMinor: 0,
};

function encoded(bytes = new Uint8Array([1, 2, 3])): P.EncodedContent {
  return {
    type,
    parameters: new Map([["key", "value"]]),
    fallback: undefined,
    content: bytes,
  };
}

describe("shared public value projection", () => {
  it("projects nested content variants and preserves byte views", () => {
    const padded = new Uint8Array([99, 1, 2, 3, 88]);
    const value: P.MessageContent = {
      kind: "reply",
      referenceId: "message-id",
      body: {
        kind: "custom",
        encoded: encoded(padded.subarray(1, 4)),
        rawBytes: padded.subarray(1, 4),
      },
    };
    const raw = P.lowerMessageContent(value, projection);
    expect(raw.tag).toBe(B.MessageContent_Tags.Reply);
    if (raw.tag !== B.MessageContent_Tags.Reply)
      throw new Error("reply missing");
    expect(raw.inner.body.tag).toBe(B.MessageBody_Tags.Custom);
    if (raw.inner.body.tag !== B.MessageBody_Tags.Custom)
      throw new Error("custom missing");
    expect([...new Uint8Array(raw.inner.body.inner.encoded.content)]).toEqual([
      1, 2, 3,
    ]);
    expect([...padded]).toEqual([99, 1, 2, 3, 88]);
    expect(P.liftMessageContent(raw, projection)).toEqual(value);
    expect(P.liftMessageContent(raw, projection)).not.toHaveProperty("tag");
  });

  it("preserves bigint, Timestamp, maps, optional values, and flat enums", () => {
    const timestamp = new P.Timestamp(9007199254740993n);
    const record: P.LogRecord = {
      level: "warn",
      target: "test",
      message: "record",
      fields: new Map([["field", "value"]]),
      timestamp,
      droppedRecords: 9007199254740997n,
    };
    const raw = P.lowerLogRecord(record, projection);
    expect(raw.level).toBe(B.LogLevel.Warn);
    expect(raw.timestamp).toBe(timestamp);
    expect(raw.droppedRecords).toBe(9007199254740997n);
    expect(P.liftLogRecord(raw, projection)).toEqual(record);
    expect(
      P.liftEncodedContent(
        P.lowerEncodedContent(encoded(), projection),
        projection,
      ),
    ).toEqual(encoded());
  });

  it("projects callback inputs and results in both directions", async () => {
    const padded = new Uint8Array([99, 2, 3, 88]);
    const signer: P.Signer = {
      async identity() {
        return { kind: "ethereum", identifier: "identity" };
      },
      async kind() {
        return {
          kind: "scw",
          chainId: 9007199254740995n,
          blockNumber: undefined,
        };
      },
      async sign(request) {
        expect(request).toEqual({ text: "sign this" });
        return { kind: "ecdsa", value: padded.subarray(1, 3) };
      },
    };
    const raw = P.lowerSigner(signer, projection);
    expect(await raw.identity()).toEqual({
      kind: B.PublicIdentityKind.Ethereum,
      identifier: "identity",
    });
    expect((await raw.kind()).tag).toBe(B.SignerKind_Tags.Scw);
    const signature = await raw.sign({ text: "sign this" });
    expect(signature.tag).toBe(B.Signature_Tags.Ecdsa);
    if (signature.tag !== B.Signature_Tags.Ecdsa)
      throw new Error("signature missing");
    expect([...new Uint8Array(signature.inner[0])]).toEqual([2, 3]);
    const restored = P.liftSigner(raw, projection);
    expect(await restored.identity()).toEqual(await signer.identity());
    expect(await restored.kind()).toEqual(await signer.kind());
    expect(await restored.sign({ text: "sign this" })).toEqual(
      await signer.sign({ text: "sign this" }),
    );
  });

  it("uses one credentials field for a value or a source", async () => {
    const credential: P.Credential = {
      name: undefined,
      value: "token",
      expiresAtSeconds: 9007199254740997n,
    };
    const options: P.BackendOptions = {
      url: "https://example.test",
      credentials: credential,
    };
    const raw = P.lowerBackendOptions(options, projection);
    expect(raw.credential).toEqual(credential);
    expect(raw.credentials).toBeUndefined();
    expect(P.liftBackendOptions(raw, projection)).toEqual(options);
    const source: P.CredentialSource = {
      async credential() {
        return credential;
      },
    };
    const callback = P.lowerBackendOptions(
      { ...options, credentials: source },
      projection,
    );
    expect(callback.credential).toBeUndefined();
    expect(await callback.credentials?.credential()).toEqual(credential);
    const restored = P.liftBackendOptions(callback, projection).credentials;
    expect(
      restored && "credential" in restored && (await restored.credential()),
    ).toEqual(credential);
  });

  it("preserves backend handles and plain storage input unions", () => {
    const raw = P.lowerBackendSource(backend, projection);
    expect(raw.tag).toBe(B.BackendSource_Tags.Connected);
    expect(P.liftBackendSource(raw, projection)).toBe(backend);
    const options: P.BackendOptions = { url: "https://example.test" };
    expect(
      P.liftBackendSource(
        P.lowerBackendSource(options, projection),
        projection,
      ),
    ).toEqual(options);
    for (const location of [
      "default",
      "inMemory",
      { directory: "folder" },
      { dbPath: "db.sqlite3", attachmentsDir: "attachments" },
    ] satisfies P.StorageLocation[]) {
      expect(
        P.liftStorageLocation(
          P.lowerStorageLocation(location, projection),
          projection,
        ),
      ).toEqual(location);
    }
  });

  // verifies: ATCH-082
  it("rejects an incomplete storage location with a storage-location error", () => {
    // Plain JavaScript callers can pass any value, so these skip the type.
    const malformed: unknown[] = [
      { dbPath: "db.sqlite3" },
      { attachmentsDir: "attachments" },
      { dbPath: "", attachmentsDir: "attachments" },
      { dbPath: "db.sqlite3", attachmentsDir: "" },
      { dbPath: "db.sqlite3", attachmentsDir: 7 },
      { directory: "" },
      { directory: undefined },
      { directory: "folder", dbPath: "db.sqlite3" },
      {},
      null,
      "folder",
    ];
    for (const location of malformed) {
      let failure: unknown;
      try {
        P.lowerStorageLocation(location as P.StorageLocation, projection);
      } catch (error) {
        failure = error;
      }
      expect(failure, JSON.stringify(location)).toBeInstanceOf(
        P.XmtpError.StorageLocation,
      );
      expect((failure as P.XmtpError).details).toMatchObject({
        code: "StorageLocation",
        category: "storage",
        retryable: false,
      });
    }
  });
  it("rejects malformed attachment sources before field conversion", () => {
    for (const source of [
      null,
      undefined,
      {},
      { kind: "unknown" },
      { kind: "path" },
      { kind: "path", path: undefined },
      { kind: "path", path: 7 },
      { kind: "path", path: "source", bytes: undefined },
      { kind: "bytes" },
      { kind: "bytes", bytes: undefined },
      { kind: "bytes", bytes: [1, 2] },
      { kind: "bytes", bytes: new Uint8Array([1]), path: undefined },
    ]) {
      let failure: unknown;
      try {
        P.lowerAttachmentSource(source as P.AttachmentSource, projection);
      } catch (error) {
        failure = error;
      }
      expect(failure).toBeInstanceOf(P.XmtpError.Attachment);
      const error = failure as InstanceType<typeof P.XmtpError.Attachment>;
      expect(error.details).toMatchObject({
        code: "Attachment",
        category: "input",
        retryable: false,
      });
      expect(error.attachmentFailure).toEqual({
        cause: "malformed",
        credentialKind: undefined,
        retryable: false,
        missingScope: false,
        httpStatus: undefined,
      });
    }
    for (const source of [
      {
        kind: "bytes",
        bytes: new Uint8Array(),
        filename: undefined,
        mimeType: "text/plain",
      },
      {
        kind: "path",
        path: "source",
        filename: "name",
        mimeType: "text/plain",
      },
    ] satisfies P.AttachmentSource[]) {
      expect(
        P.liftAttachmentSource(
          P.lowerAttachmentSource(source, projection),
          projection,
        ),
      ).toEqual(source);
    }
  });
});
