import { MantineProvider } from "@mantine/core";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { Client, IdentifierKind } from "@xmtp/browser-sdk";
import { MemoryRouter } from "react-router";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { InboxTools } from "./InboxTools";

const mocks = vi.hoisted(() => ({
  inboxId: "a".repeat(64),
  backendUrl: "https://one.example",
  authCallback: vi.fn(),
  signMessageAsync: vi.fn(),
}));
vi.mock("@/hooks/useWallet", () => ({
  useWallet: () => ({ address: "0x1234", isConnected: true }),
}));
vi.mock("@/hooks/useEphemeralSigner", () => ({
  useEphemeralSigner: () => ({}),
}));
vi.mock("wagmi", () => ({
  useSignMessage: () => ({ signMessageAsync: mocks.signMessageAsync }),
}));
vi.mock("@/hooks/useMemberId", () => ({
  useMemberId: () => ({
    inboxId: mocks.inboxId,
    memberId: mocks.inboxId,
    setMemberId: vi.fn(),
  }),
}));
vi.mock("@/hooks/useSettings", () => ({
  useSettings: () => ({ backendUrl: mocks.backendUrl }),
}));
vi.mock("@/contexts/AuthTokenContext", () => ({
  useAuthToken: () => ({ createAuthCallback: () => mocks.authCallback }),
}));
vi.mock("@/helpers/backend", () => ({
  backendLabel: () => Promise.resolve("local"),
}));
vi.mock("@/components/App/BackendUrlInput", () => ({
  BackendUrlInput: () => null,
}));
vi.mock("@/components/App/ConnectedAddress", () => ({
  ConnectedAddress: () => null,
}));
vi.mock("@/components/App/WalletConnect", () => ({
  WalletConnect: () => null,
}));
vi.mock("@/layouts/ContentLayout", () => ({
  ContentLayout: ({
    children,
    footer,
  }: {
    children: React.ReactNode;
    footer: React.ReactNode;
  }) => (
    <>
      {children}
      {footer}
    </>
  ),
}));

const view = () => (
  <MantineProvider>
    <MemoryRouter>
      <InboxTools />
    </MemoryRouter>
  </MantineProvider>
);

beforeEach(() => {
  mocks.inboxId = "a".repeat(64);
  mocks.backendUrl = "https://one.example";
  vi.restoreAllMocks();
});

describe("InboxTools query state", () => {
  const installation = {
    id: "installation-1",
    bytes: new Uint8Array([1]),
    clientTimestampNs: 1n,
  };
  const inboxState = {
    inboxId: mocks.inboxId,
    installations: [installation],
    accountIdentifiers: [],
    recoveryIdentifier: {
      identifier: "0x1234",
      identifierKind: IdentifierKind.Ethereum,
    },
  };

  it("resets results for a new inbox without replacing the input", async () => {
    vi.spyOn(Client, "fetchLatestInboxUpdatesCount").mockResolvedValue(
      new Map([[mocks.inboxId, 123]]),
    );
    const { rerender, unmount } = render(view());
    const input = screen.getByRole("textbox");
    fireEvent.click(
      screen.getByRole("button", { name: "Check updates count" }),
    );
    await screen.findByText("123");
    mocks.inboxId = "b".repeat(64);
    rerender(view());
    expect(screen.getByText("No count fetched")).toBeTruthy();
    expect(screen.getByRole("textbox")).toBe(input);
    unmount();
  });

  it("ignores a late response from the previous backend", async () => {
    const pending = Promise.withResolvers<Map<string, number>>();
    const fetch = vi
      .spyOn(Client, "fetchLatestInboxUpdatesCount")
      .mockReturnValueOnce(pending.promise)
      .mockResolvedValueOnce(new Map([[mocks.inboxId, 456]]));
    const { rerender, unmount } = render(view());
    fireEvent.click(
      screen.getByRole("button", { name: "Check updates count" }),
    );
    await waitFor(() => {
      expect(fetch).toHaveBeenCalledTimes(1);
    });
    mocks.backendUrl = "https://two.example";
    rerender(view());
    fireEvent.click(
      screen.getByRole("button", { name: "Check updates count" }),
    );
    await screen.findByText("456");
    await act(async () => {
      pending.resolve(new Map([[mocks.inboxId, 123]]));
      await pending.promise;
    });
    expect(screen.getByText("456")).toBeTruthy();
    expect(screen.queryByText("123")).toBeNull();
    unmount();
  });

  it("clears the selection when refresh fails after a successful revoke", async () => {
    const fetch = vi
      .spyOn(Client, "fetchInboxStates")
      .mockResolvedValueOnce([inboxState])
      .mockRejectedValueOnce(new Error("refresh failed"));
    const revoke = vi
      .spyOn(Client, "revokeInstallations")
      .mockResolvedValue(undefined);
    const consoleError = vi
      .spyOn(console, "error")
      .mockImplementation(() => undefined);

    const { unmount } = render(view());
    fireEvent.click(screen.getByRole("button", { name: "Find installations" }));
    await screen.findByText(installation.id);
    fireEvent.click(screen.getByRole("checkbox"));
    const revokeButton = screen.getByRole("button", {
      name: "Revoke installations",
    });
    expect(revokeButton).not.toBeDisabled();
    fireEvent.click(revokeButton);

    await waitFor(() => {
      expect(revoke).toHaveBeenCalledOnce();
      expect(fetch).toHaveBeenCalledTimes(2);
      expect(revokeButton).toBeDisabled();
      expect(consoleError).toHaveBeenCalledWith(expect.any(Error));
    });
    unmount();
  });

  it("keeps the selection and logs when revoke fails", async () => {
    vi.spyOn(Client, "fetchInboxStates").mockResolvedValue([inboxState]);
    const revoke = vi
      .spyOn(Client, "revokeInstallations")
      .mockRejectedValue(new Error("revoke failed"));
    const consoleError = vi
      .spyOn(console, "error")
      .mockImplementation(() => undefined);

    const { unmount } = render(view());
    fireEvent.click(screen.getByRole("button", { name: "Find installations" }));
    await screen.findByText(installation.id);
    fireEvent.click(screen.getByRole("checkbox"));
    const revokeButton = screen.getByRole("button", {
      name: "Revoke installations",
    });
    fireEvent.click(revokeButton);

    await waitFor(() => {
      expect(revoke).toHaveBeenCalledOnce();
      expect(revokeButton).not.toBeDisabled();
      expect(consoleError).toHaveBeenCalledWith(expect.any(Error));
    });
    unmount();
  });
});
