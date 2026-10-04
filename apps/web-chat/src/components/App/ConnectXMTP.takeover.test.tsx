import { MantineProvider } from "@mantine/core";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";

import { ConnectXMTP } from "./ConnectXMTP";

const mocks = vi.hoisted(() => ({ connect: vi.fn() }));
vi.mock("@/contexts/XMTPContext", () => ({
  useXMTP: () => ({ lockState: "active" }),
}));
vi.mock("@/hooks/useConnectXmtp", () => ({
  useConnectXmtp: () => ({ connect: mocks.connect, loading: false }),
}));
vi.mock("@/hooks/useWallet", () => ({
  useWallet: () => ({ isConnected: false, disconnect: vi.fn() }),
}));
vi.mock("@/hooks/useEphemeralSigner", () => ({
  useEphemeralSigner: () => ({}),
}));
vi.mock("@/hooks/useSettings", () => ({
  useSettings: () => ({
    backendUrl: "https://example.com",
    ephemeralAccountEnabled: true,
    setEphemeralAccountEnabled: vi.fn(),
  }),
}));
vi.mock("@/components/App/AppLockModal", () => ({
  AppLockModal: () => null,
}));
vi.mock("@/components/App/AppLockDisconnectModal", () => ({
  AppLockDisconnectModal: () => null,
}));
vi.mock("@/components/App/AuthTokenInput", () => ({
  AuthTokenInput: () => null,
}));
vi.mock("@/components/App/BackendUrlInput", () => ({
  BackendUrlInput: () => null,
}));
vi.mock("@/components/App/ConnectedAddress", () => ({
  ConnectedAddress: () => null,
}));
vi.mock("@/components/App/LoggingSelect", () => ({
  LoggingSelect: () => null,
}));
vi.mock("./LocalDatabases", () => ({ LocalDatabases: () => null }));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

it("connects after this tab takes the app lock", () => {
  render(
    <MantineProvider>
      <ConnectXMTP />
    </MantineProvider>,
  );

  fireEvent.click(screen.getByRole("button", { name: "Connect" }));

  expect(mocks.connect).toHaveBeenCalledOnce();
});
