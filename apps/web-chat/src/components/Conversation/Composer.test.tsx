import { MantineProvider } from "@mantine/core";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";

import { Composer } from "./Composer";

const mocks = vi.hoisted(() => ({
  upload: vi.fn(),
  deleteLocal: vi.fn(async () => {}),
  sendRemoteAttachment: vi.fn(),
  sendReply: vi.fn(),
  sendText: vi.fn(),
  setReplyTarget: vi.fn(),
}));

vi.mock("@/contexts/XMTPContext", () => ({
  useClient: () => ({
    attachments: { offered: true, deleteLocal: mocks.deleteLocal },
  }),
}));
vi.mock("@/contexts/ConversationContext", () => ({
  useConversationContext: () => ({
    replyTarget: undefined,
    setReplyTarget: mocks.setReplyTarget,
  }),
}));
vi.mock("@/hooks/useConversation", () => ({
  useConversation: () => ({
    sendRemoteAttachment: mocks.sendRemoteAttachment,
    sendReply: mocks.sendReply,
    sendText: mocks.sendText,
    sending: false,
  }),
}));
vi.mock("@/helpers/attachment", async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  uploadEncryptedAttachment: mocks.upload,
  validateFile: () => ({ valid: true }),
}));
vi.mock("./AttachmentPreview", () => ({
  AttachmentPreview: ({ onCancel }: { onCancel: () => void }) => (
    <button type="button" onClick={onCancel}>
      Cancel attachment
    </button>
  ),
}));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

const selectFile = (name: string) => {
  const input = document.querySelector('input[type="file"]');
  if (!(input instanceof HTMLInputElement)) throw new Error("No file input");
  const file = new File([name], name, { type: "image/png" });
  fireEvent.change(input, { target: { files: [file] } });
  return file;
};

it("reuses the staged attachment after an upload failure", async () => {
  const pending = { remoteAttachment: { url: "https://example.com/retry" } };
  mocks.upload
    .mockImplementationOnce(async (_client, _file, ref) => {
      ref.current = pending;
      throw new Error("network failed");
    })
    .mockImplementationOnce(async (_client, _file, ref) => {
      expect(ref.current).toBe(pending);
      return pending.remoteAttachment;
    });
  render(
    <MantineProvider>
      <Composer conversationId="conversation" />
    </MantineProvider>,
  );

  const file = selectFile("retry.png");
  fireEvent.click(screen.getByRole("button", { name: "Send" }));
  await waitFor(() =>
    expect(screen.getByText("Failed to upload attachment")).toBeVisible(),
  );
  fireEvent.click(screen.getByRole("button", { name: "Send" }));
  await waitFor(() =>
    expect(mocks.sendRemoteAttachment).toHaveBeenCalledWith(
      pending.remoteAttachment,
    ),
  );
  expect(mocks.upload).toHaveBeenNthCalledWith(
    2,
    expect.anything(),
    file,
    expect.objectContaining({ current: null }),
  );
});

for (const change of ["replace", "cancel"] as const) {
  it(`uploads the selected file after a failed send and ${change}`, async () => {
    const firstRemote = { url: "https://example.com/first" };
    const secondRemote = { url: "https://example.com/second" };
    mocks.upload
      .mockResolvedValueOnce(firstRemote)
      .mockResolvedValueOnce(secondRemote);
    mocks.sendRemoteAttachment
      .mockRejectedValueOnce(new Error("send failed"))
      .mockResolvedValueOnce(undefined);
    render(
      <MantineProvider>
        <Composer conversationId="conversation" />
      </MantineProvider>,
    );

    const first = selectFile("first.png");
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() =>
      expect(mocks.sendRemoteAttachment).toHaveBeenCalledWith(firstRemote),
    );
    if (change === "cancel") {
      fireEvent.click(
        screen.getByRole("button", { name: "Cancel attachment" }),
      );
    }
    const second = selectFile("second.png");
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() =>
      expect(mocks.sendRemoteAttachment).toHaveBeenLastCalledWith(secondRemote),
    );
    expect(mocks.upload).toHaveBeenNthCalledWith(
      1,
      expect.anything(),
      first,
      expect.anything(),
    );
    expect(mocks.upload).toHaveBeenNthCalledWith(
      2,
      expect.anything(),
      second,
      expect.anything(),
    );
  });
}
