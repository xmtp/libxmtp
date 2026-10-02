import { mkdtemp, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  decodeEncodedContent,
  decryptEncodedContent,
  AttachmentCodec,
  XmtpError,
  type Client,
  type RemoteAttachment,
} from "@xmtp/node-sdk";
import { describe, expect, it, vi } from "vitest";

import {
  createRemoteAttachmentFromFile,
  createRemoteAttachment,
  downloadRemoteAttachment,
  type HostedAttachment,
} from "./AttachmentUtil";

const remote = {
  url: "https://storage.example/file",
  scheme: "https://",
  contentDigest: "digest",
  secret: new Uint8Array(32),
  salt: new Uint8Array(32),
  nonce: new Uint8Array(12),
  filename: "hello.txt",
} satisfies RemoteAttachment;
describe("AttachmentUtil", () => {
  const hosted = {
    payload: new Uint8Array([1, 2, 3]),
    secret: new Uint8Array(32),
    salt: new Uint8Array(32),
    nonce: new Uint8Array(12),
    digest: "unverified app digest",
    length: 999n,
    filename: "ciphertext.bin",
  } satisfies HostedAttachment;

  it.each([
    ["https://storage.example/file", "https://"],
    ["http://127.0.0.1:5050/file", "http://"],
  ])("uses Rust URL and ciphertext checks for %s", (url, scheme) => {
    const result = createRemoteAttachment(hosted, url);
    expect(result).toEqual({
      url,
      scheme,
      contentDigest:
        "039058c6f2c0cb492c533b0a4d14ef77cc0f78abccced5287d84a1a2011cfb81",
      contentLength: 3,
      filename: hosted.filename,
      secret: hosted.secret,
      salt: hosted.salt,
      nonce: hosted.nonce,
    });
  });

  it.each([
    "ftp://storage.example/file",
    "http://storage.example/file",
    "not a URL",
  ])("returns the Rust typed URL rejection for %s", (url) => {
    let failure: unknown;
    try {
      createRemoteAttachment(hosted, url);
    } catch (error) {
      failure = error;
    }
    expect(failure).toBeInstanceOf(XmtpError.InvalidArgument);
    expect(failure).toMatchObject({
      details: {
        code: "InvalidArgument",
        category: "input",
        retryable: false,
      },
    });
  });

  it("uses the configured Rust attachment upload", async () => {
    const upload = vi.fn(async () => undefined);
    const create = vi.fn(async () => ({ upload, remoteAttachment: remote }));
    const client = { attachments: { create } } as unknown as Client;
    const file = new File(["hello"], "hello.txt", { type: "text/plain" });
    expect(await createRemoteAttachmentFromFile(client, file)).toBe(remote);
    expect(create).toHaveBeenCalledWith({
      kind: "bytes",
      bytes: new TextEncoder().encode("hello"),
      filename: "hello.txt",
      mimeType: "text/plain",
    });
    expect(upload).toHaveBeenCalledOnce();
  });
  it("retains app hosting and round trips the Rust encrypted envelope", async () => {
    const client = {} as Client;
    const file = new File(["hello"], "hello.txt", { type: "text/plain" });
    let decoded;
    const callback = vi.fn(async (hosted) => {
      const bytes = await decryptEncodedContent({
        ciphertext: hosted.payload,
        keys: hosted,
      });
      decoded = new AttachmentCodec().decode(decodeEncodedContent(bytes));
      return remote.url;
    });
    const result = await createRemoteAttachmentFromFile(client, file, callback);
    expect(callback).toHaveBeenCalledOnce();
    expect(result.url).toBe(remote.url);
    expect(result.scheme).toBe("https://");
    expect(result.contentDigest).toBe(callback.mock.calls[0]![0].digest);
    expect(decoded).toEqual({
      filename: file.name,
      mimeType: file.type,
      content: new TextEncoder().encode("hello"),
    });
  });
  it("reads the verified file from Rust download", async () => {
    const directory = await mkdtemp(join(tmpdir(), "xmtp-agent-attachment-"));
    try {
      const path = join(directory, "hello.txt");
      await writeFile(path, "hello");
      const download = vi.fn(async () => ({
        path,
        filename: "hello.txt",
        mimeType: "text/plain",
      }));
      const client = { attachments: { download } } as unknown as Client;
      expect(await downloadRemoteAttachment(client, remote)).toEqual({
        filename: "hello.txt",
        mimeType: "text/plain",
        content: new TextEncoder().encode("hello"),
      });
      expect(download).toHaveBeenCalledWith(remote);
    } finally {
      await rm(directory, { recursive: true, force: true });
    }
  });
});
