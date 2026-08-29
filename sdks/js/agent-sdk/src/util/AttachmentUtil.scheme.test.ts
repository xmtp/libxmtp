import { describe, expect, it, vi } from "vitest";

vi.mock("@xmtp/node-sdk", () => ({
  decryptAttachment: vi.fn(),
  encryptAttachment: vi.fn(),
}));

import { createRemoteAttachment } from "@/util/AttachmentUtil";

describe("createRemoteAttachment", () => {
  it("stores URL scheme without a trailing colon", () => {
    const remoteAttachment = createRemoteAttachment(
      {
        contentDigest: new Uint8Array([1, 2, 3]),
        filename: "hello.txt",
        nonce: new Uint8Array([4, 5, 6]),
        payload: new Uint8Array([7, 8, 9]),
        salt: new Uint8Array([10, 11, 12]),
        secret: new Uint8Array([13, 14, 15]),
      },
      "https://localhost/test_file",
    );

    expect(remoteAttachment.scheme).toBe("https");
  });
});
