import { MantineProvider } from "@mantine/core";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { Client } from "@xmtp/browser-sdk";
import { MemoryRouter } from "react-router";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { InboxTools } from "./InboxTools";

const mocks = vi.hoisted(() => ({
  inboxId: "a".repeat(64),
  backendUrl: "https://one.example",
  authCallback: vi.fn(),
}));
vi.mock("@/hooks/useWallet", () => ({ useWallet: () => ({}) }));
vi.mock("@/hooks/useEphemeralSigner", () => ({
  useEphemeralSigner: () => ({}),
}));
vi.mock("wagmi", () => ({ useSignMessage: () => ({}) }));
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
  ContentLayout: ({ children }: { children: React.ReactNode }) => children,
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
});
