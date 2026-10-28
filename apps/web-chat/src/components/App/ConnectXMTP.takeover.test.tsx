import { MantineProvider } from "@mantine/core";
import {
  cleanup,
  fireEvent,
  render,
  renderHook,
  screen,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";

import { useConnectXmtp } from "@/hooks/useConnectXmtp";

import { ConnectXMTP } from "./ConnectXMTP";

const mocks = vi.hoisted(() => ({
  lockState: "active",
  initialize: vi.fn(async () => undefined),
  signer: {},
  backendUrl: "https://example.com",
  setAutoConnect: vi.fn(),
  navigate: vi.fn(),
}));
vi.mock("@/contexts/XMTPContext", () => ({
  useXMTP: () => ({
    lockState: mocks.lockState,
    initialize: mocks.initialize,
    initializing: false,
  }),
}));
vi.mock("react-router", () => ({ useNavigate: () => mocks.navigate }));
vi.mock("wagmi", () => ({
  useAccount: () => ({}),
  useSignMessage: () => ({ signMessageAsync: vi.fn() }),
}));
vi.mock("@/contexts/AuthTokenContext", () => ({
  useAuthToken: () => ({
    createCredentialSource: (url: string) => ({ url }),
  }),
}));
vi.mock("@/hooks/useWallet", () => ({
  useWallet: () => ({ isConnected: false, disconnect: vi.fn() }),
}));
vi.mock("@/hooks/useEphemeralSigner", () => ({
  useEphemeralSigner: () => ({ signer: mocks.signer }),
}));
vi.mock("@/hooks/useSettings", () => ({
  useSettings: () => ({
    backendUrl: mocks.backendUrl,
    ephemeralAccountEnabled: true,
    setEphemeralAccountEnabled: vi.fn(),
    autoConnect: false,
    setAutoConnect: mocks.setAutoConnect,
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
  mocks.backendUrl = "https://example.com";
});

it.each(["available", "active"])(
  "connects through the real hook when the lock is %s",
  (lockState) => {
    mocks.lockState = lockState;
    render(
      <MantineProvider>
        <ConnectXMTP />
      </MantineProvider>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Connect" }));

    expect(mocks.initialize).toHaveBeenCalledExactlyOnceWith({
      authCallback: { url: "https://example.com" },
      backendUrl: "https://example.com",
      loggingLevel: undefined,
      signer: mocks.signer,
    });
  },
);

it("does not initialize when another tab owns the lock", () => {
  mocks.lockState = "locked";
  const { result } = renderHook(useConnectXmtp);
  result.current.connect();
  expect(mocks.initialize).not.toHaveBeenCalled();
});

it("binds the credential source to the backend it connects to", () => {
  mocks.lockState = "available";
  const { result, rerender } = renderHook(useConnectXmtp);
  mocks.backendUrl = "https://two.example";
  rerender();
  result.current.connect();
  expect(mocks.initialize).toHaveBeenCalledExactlyOnceWith(
    expect.objectContaining({ authCallback: { url: "https://two.example" } }),
  );
});
