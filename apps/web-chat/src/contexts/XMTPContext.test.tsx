import { act, cleanup, renderHook } from "@testing-library/react";
import { Storage, type Client } from "@xmtp/browser-sdk";
import { generatePrivateKey } from "viem/accounts";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { isCurrentDatabasePath } from "@/helpers/attachment";
import { createEphemeralSigner } from "@/helpers/createSigner";

import { XMTPProvider, useXMTP } from "./XMTPContext";

describe("XMTPProvider", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    cleanup();
    localStorage.clear();
  });

  it("initializes an ephemeral client against XMTP_BACKEND_URL", async () => {
    const backendUrl = import.meta.env.XMTP_BACKEND_URL;
    if (!backendUrl) {
      throw new Error(
        "XMTP_BACKEND_URL must be defined for the XMTP smoke test",
      );
    }
    const { result } = renderHook(() => useXMTP(), { wrapper: XMTPProvider });
    const initialized: { client?: Client } = {};
    await act(async () => {
      initialized.client = await result.current.initialize({
        backendUrl,
        signer: createEphemeralSigner(generatePrivateKey()),
      });
    });
    expect(initialized.client?.inboxId).toBeTruthy();
    await initialized.client?.end();
  });

  it("preserves completed downloads and cleans temporary files before another inbox starts", async () => {
    const backendUrl = import.meta.env.XMTP_BACKEND_URL;
    if (!backendUrl) throw new Error("XMTP_BACKEND_URL is required");
    const first = renderHook(() => useXMTP(), { wrapper: XMTPProvider });
    let firstPath: string | undefined;
    let secondPath: string | undefined;
    let attachmentDirectory: FileSystemDirectoryHandle | undefined;
    try {
      let firstClient: Client | undefined;
      await act(async () => {
        firstClient = await first.result.current.initialize({
          backendUrl,
          signer: createEphemeralSigner(generatePrivateKey()),
        });
      });
      firstPath = await firstClient?.storage.path();
      if (!firstPath || !isCurrentDatabasePath(firstPath)) {
        throw new Error(`Unexpected Browser storage path: ${firstPath}`);
      }
      await act(async () => {
        await first.result.current.disconnect();
      });
      first.unmount();

      const root = await navigator.storage.getDirectory();
      let inbox = root;
      for (const part of firstPath.split("/").slice(0, -1)) {
        inbox = await inbox.getDirectoryHandle(part);
      }
      attachmentDirectory = await inbox.getDirectoryHandle("attachments", {
        create: true,
      });
      const plaintext = "a".repeat(64);
      const completed = await attachmentDirectory.getDirectoryHandle(
        plaintext,
        { create: true },
      );
      const downloaded = await completed.getFileHandle("download", {
        create: true,
      });
      const writer = await downloaded.createWritable();
      await writer.write("retained download");
      await writer.close();
      await attachmentDirectory.getDirectoryHandle(".tmp", { create: true });
      const staged = await attachmentDirectory.getDirectoryHandle(".staged", {
        create: true,
      });
      await staged.getFileHandle("ciphertext", { create: true });

      const second = renderHook(() => useXMTP(), { wrapper: XMTPProvider });
      try {
        let secondClient: Client | undefined;
        await act(async () => {
          secondClient = await second.result.current.initialize({
            backendUrl,
            signer: createEphemeralSigner(generatePrivateKey()),
          });
        });
        secondPath = await secondClient?.storage.path();
        await expect(
          attachmentDirectory.getDirectoryHandle(".tmp"),
        ).rejects.toMatchObject({ name: "NotFoundError" });
        expect(await staged.getFileHandle("ciphertext")).toBeDefined();
        expect(await (await downloaded.getFile()).text()).toBe(
          "retained download",
        );
        const admin = await Storage.admin();
        try {
          expect(await admin.fileExists(firstPath)).toBe(true);
        } finally {
          await admin.end();
        }
      } finally {
        await act(async () => {
          await second.result.current.disconnect();
        });
        second.unmount();
      }
    } finally {
      first.unmount();
      if (attachmentDirectory) {
        for await (const [name] of attachmentDirectory.entries()) {
          await attachmentDirectory.removeEntry(name, { recursive: true });
        }
      }
      const admin = await Storage.admin();
      try {
        for (const path of [firstPath, secondPath]) {
          if (path) await admin.deleteFile(path);
        }
      } finally {
        await admin.end();
      }
    }
  });
});
