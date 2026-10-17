import { MantineProvider } from "@mantine/core";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";

import { AppLockModal } from "./AppLockModal";

const mocks = vi.hoisted(() => ({
  acquireLock: vi.fn(),
  releaseLock: vi.fn(),
  setAutoConnect: vi.fn(),
}));
vi.mock("@/contexts/XMTPContext", () => ({
  useXMTP: () => ({
    acquireLock: mocks.acquireLock,
    releaseLock: mocks.releaseLock,
  }),
}));
vi.mock("@/hooks/useSettings", () => ({
  useSettings: () => ({ setAutoConnect: mocks.setAutoConnect }),
}));
vi.mock("@/hooks/useCollapsedMediaQuery", () => ({
  useCollapsedMediaQuery: () => false,
}));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

it("claims the lock from the other tab when the user requests takeover", () => {
  const onClose = vi.fn();
  const onDisconnect = vi.fn();
  render(
    <MantineProvider>
      <AppLockModal opened onClose={onClose} onDisconnect={onDisconnect} />
    </MantineProvider>,
  );

  fireEvent.click(
    screen.getByRole("button", { name: "Disconnect other session" }),
  );

  expect(mocks.acquireLock).toHaveBeenCalledExactlyOnceWith(true);
  expect(mocks.releaseLock).not.toHaveBeenCalled();
  expect(mocks.setAutoConnect).toHaveBeenCalledExactlyOnceWith(false);
  expect(onClose).toHaveBeenCalledOnce();
  expect(onDisconnect).toHaveBeenCalledOnce();
});
