import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useParams } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { LoadDM } from "./LoadDM";

const mocks = vi.hoisted(() => {
  const client = {
    canMessage: vi.fn(),
    inboxIdFor: vi.fn(),
    conversations: {
      getDmByInboxId: vi.fn(),
      createDm: vi.fn(),
    },
  };
  return { client, addConversation: vi.fn() };
});
vi.mock("@/contexts/XMTPContext", () => ({
  useClient: () => mocks.client,
}));
vi.mock("@/stores/inbox/hooks", () => ({
  useActions: () => ({ addConversation: mocks.addConversation }),
}));
vi.mock("@/components/LoadingMessage", () => ({
  LoadingMessage: ({ message }: { message: string }) => <p>{message}</p>,
}));

const OpenedConversation = () => {
  const { conversationId } = useParams();
  return <p>conversation {conversationId}</p>;
};

const open = (url: string) =>
  render(
    <MemoryRouter initialEntries={[url]}>
      <Routes>
        <Route path="/dm/:address" element={<LoadDM />} />
        <Route
          path="/conversations/:conversationId"
          element={<OpenedConversation />}
        />
      </Routes>
    </MemoryRouter>,
  );

const address = `0x${"a".repeat(40)}`;

beforeEach(() => {
  vi.clearAllMocks();
  mocks.client.canMessage.mockResolvedValue(
    new Map([[`ethereum:${address}`, true]]),
  );
  mocks.client.inboxIdFor.mockResolvedValue("b".repeat(64));
  mocks.client.conversations.getDmByInboxId.mockResolvedValue({ id: "dm-1" });
});

afterEach(() => {
  cleanup();
});

describe("LoadDM", () => {
  it("opens the DM for a coded handle, whose code is the URL fragment", async () => {
    open("/dm/nick#2341");
    await screen.findByText("conversation dm-1");
    expect(mocks.client.inboxIdFor).toHaveBeenCalledWith({
      identifier: "nick#2341",
      kind: "handle",
    });
  });

  it("refuses a fragment that is not a handle code", async () => {
    open("/dm/nick#12");
    await screen.findByText("Invalid identifier, redirecting...");
    expect(mocks.client.inboxIdFor).not.toHaveBeenCalled();
  });

  it("opens the DM for an address and ignores its fragment", async () => {
    open(`/dm/${address}#ref`);
    await screen.findByText("conversation dm-1");
    expect(mocks.client.inboxIdFor).toHaveBeenCalledWith({
      identifier: address,
      kind: "ethereum",
    });
  });
});
