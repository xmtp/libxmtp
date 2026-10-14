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
    // the coded handle, never the bare name before the fragment
    expect(mocks.client.inboxIdFor).toHaveBeenCalledExactlyOnceWith({
      identifier: "nick#2341",
      kind: "handle",
    });
  });

  it("resolves a handle through the SDK and creates the DM by its resolved inbox ID", async () => {
    const handle = { identifier: "nick", kind: "handle" };
    const newDm = { id: "dm-2" };
    mocks.client.conversations.getDmByInboxId.mockResolvedValue(undefined);
    mocks.client.conversations.createDm.mockResolvedValue(newDm);

    open("/dm/Nick");
    await screen.findByText("conversation dm-2");
    expect(mocks.client.canMessage).not.toHaveBeenCalled();
    expect(mocks.client.inboxIdFor).toHaveBeenCalledExactlyOnceWith(handle);
    expect(mocks.client.conversations.getDmByInboxId).toHaveBeenCalledWith(
      "b".repeat(64),
    );
    expect(mocks.client.conversations.createDm).toHaveBeenCalledWith(
      "b".repeat(64),
    );
    expect(mocks.addConversation).toHaveBeenCalledWith(newDm);
  });

  it("refuses a handle that no inbox holds", async () => {
    mocks.client.inboxIdFor.mockResolvedValue(undefined);

    open("/dm/nobody");
    await screen.findByText(
      "No inbox has this handle on this server, redirecting...",
    );
    expect(mocks.client.conversations.createDm).not.toHaveBeenCalled();
  });

  it("lowercases ASCII capitals in the name and the fragment", async () => {
    open("/dm/Nick#2341");
    await screen.findByText("conversation dm-1");
    expect(mocks.client.inboxIdFor).toHaveBeenCalledExactlyOnceWith({
      identifier: "nick#2341",
      kind: "handle",
    });
  });

  // toLowerCase would fold U+212A KELVIN SIGN to "k" and resolve "kate"
  it.each(["/dm/\u212Aate", "/dm/\u0130nci", "/dm/\u212Aate#2341"])(
    "refuses a link with a non-ASCII capital: %s",
    async (url) => {
      open(url);
      await screen.findByText("Invalid identifier, redirecting...");
      expect(mocks.client.inboxIdFor).not.toHaveBeenCalled();
    },
  );

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

it("refuses a short Ethereum-prefixed DM link instead of resolving a handle", async () => {
  open("/dm/0xnick");
  await screen.findByText("Invalid identifier, redirecting...");
  expect(mocks.client.inboxIdFor).not.toHaveBeenCalled();
  expect(mocks.client.conversations.createDm).not.toHaveBeenCalled();
});

it("opens a displayed @handle link by its raw coded handle", async () => {
  open("/dm/@Nick#2341");
  await screen.findByText("conversation dm-1");
  expect(mocks.client.inboxIdFor).toHaveBeenCalledExactlyOnceWith({
    identifier: "nick#2341",
    kind: "handle",
  });
});
