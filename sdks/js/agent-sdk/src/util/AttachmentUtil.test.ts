import { encryptAttachment } from "@xmtp/node-sdk";
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
  type Mock,
} from "vitest";
import {
  createRemoteAttachment,
  createRemoteAttachmentFromFile,
  downloadRemoteAttachment,
} from "@/util/AttachmentUtil";

describe("AttachmentUtil", () => {
  const testUrl = "https://localhost/test_file";
  let mockFetch: Mock;

  beforeEach(() => {
    mockFetch = vi.fn();
    global.fetch = mockFetch;
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe("createRemoteAttachmentFromFile", () => {
    it("creates a remote attachment", async () => {
      const fileContent = "createRemoteAttachmentFromFile";
      const fileName = "hello.txt";
      const mimeType = "text/plain";
      const unencryptedFile = new File([fileContent], fileName, {
        type: mimeType,
      });
      const uploadCallback = () => {
        return Promise.resolve(testUrl);
      };
      const remoteAttachment = await createRemoteAttachmentFromFile(
        unencryptedFile,
        uploadCallback,
      );
      expect(remoteAttachment.url).toBe(testUrl);
      expect(remoteAttachment.filename).toBe(fileName);
    });
  });

  describe("Round-trip test", () => {
    it("encrypts and decrypts a file", async () => {
      const fileContent = "Hello, World!";
      const fileName = "hello.txt";
      const mimeType = "text/plain";
      const unencryptedFile = new File([fileContent], fileName, {
        type: mimeType,
      });
      const arrayBuffer = await unencryptedFile.arrayBuffer();
      const attachment = new Uint8Array(arrayBuffer);

      const encryptedAttachment = encryptAttachment({
        filename: unencryptedFile.name,
        content: attachment,
        mimeType: unencryptedFile.type,
      });

      // Mock fetch to return the encrypted payload
      mockFetch.mockResolvedValueOnce({
        ok: true,
        arrayBuffer: async () =>
          Promise.resolve(encryptedAttachment.payload.buffer),
      });

      const remoteAttachment = createRemoteAttachment(
        encryptedAttachment,
        testUrl,
      );

      expect(remoteAttachment.url).toBe(testUrl);
      expect(remoteAttachment.filename).toBe(fileName);
      // Issue #4034: scheme must not carry the trailing colon that
      // URL.protocol includes (e.g. "https", not "https:"), matching the
      // format used elsewhere in libxmtp's remote attachment tests/examples.
      expect(remoteAttachment.scheme).toBe("https");

      const receivedAttachment =
        await downloadRemoteAttachment(remoteAttachment);

      // Verify fetch was called with the correct URL
      expect(mockFetch).toHaveBeenCalledWith(testUrl);

      // Verify the decrypted attachment matches the original
      expect(receivedAttachment.filename).toBe(fileName);
      expect(receivedAttachment.mimeType).toBe(mimeType);
      expect(receivedAttachment.content).toEqual(attachment);

      // Verify the content matches
      const decryptedContent = new TextDecoder().decode(
        receivedAttachment.content,
      );
      expect(decryptedContent).toBe(fileContent);
    });
  });

  describe("createRemoteAttachment scheme normalization", () => {
    it("strips the trailing colon from URL.protocol (issue #4034)", async () => {
      const fileContent = "scheme normalization test";
      const unencryptedFile = new File([fileContent], "hello.txt", {
        type: "text/plain",
      });
      const arrayBuffer = await unencryptedFile.arrayBuffer();
      const encryptedAttachment = encryptAttachment({
        filename: unencryptedFile.name,
        content: new Uint8Array(arrayBuffer),
        mimeType: unencryptedFile.type,
      });

      for (const [fileUrl, expectedScheme] of [
        ["https://localhost/test_file", "https"],
        ["http://localhost/test_file", "http"],
      ] as const) {
        const remoteAttachment = createRemoteAttachment(
          encryptedAttachment,
          fileUrl,
        );
        expect(remoteAttachment.scheme).toBe(expectedScheme);
      }
    });
  });
});
